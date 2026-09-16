//! Which reports this application can run on the server, and as whom.
//!
//! # Why this is here and not in `phonix-services`
//!
//! A definition is closed over a row type that only this crate knows, so only
//! this crate can produce a [`Rendered`] from one. The exporter lives in
//! `phonix-server`, which depends on this crate - so the worker asks here by
//! id, and what comes back has its types erased and can cross to the writers.
//!
//! # A report says what it needs
//!
//! Each entry carries the permission the report is gated on. The service that
//! raises an export is handed that name rather than reading one off the
//! request, and the worker re-checks it against the requester before it
//! renders anything. The index of reports somebody may run reads the same
//! list, which is what stops a report being reachable by one route and not
//! the other.

use phonix_core::apps::AppDescriptor;
use phonix_core::permissions;

/// One report the server can run.
pub struct ServerReport {
    /// The definition's own id.
    pub id: &'static str,
    /// What somebody must hold to run it. The definition names the same one
    /// for the screen; this is the copy the worker reads, which cannot build
    /// a definition in a const.
    pub permission: &'static str,
    /// The catalogue key the index lists it under.
    pub title: &'static str,
    /// Where it is read. A report over one record names the list it is chosen
    /// from, because the record is what supplies the rest of the address.
    pub href: &'static str,
}

impl ServerReport {
    /// Which app declares it, read from the permission rather than repeated.
    pub fn app(&self) -> Option<&'static AppDescriptor> {
        phonix_core::apps::owner_of(self.permission)
    }
}

/// Every report that can be run away from a browser.
///
/// A report missing from this list can still be read on screen; what it cannot
/// be is exported, because nothing here knows how to draw it without one.
pub const SERVER_REPORTS: &[ServerReport] = &[
    ServerReport {
        id: "product-list",
        permission: permissions::ITEMS,
        title: "items.title",
        href: "/inventory/items/report",
    },
    ServerReport {
        id: "customer-statement",
        permission: permissions::REPORTS,
        title: "reports.customer_statement",
        href: "/accounting/reports/statement",
    },
    ServerReport {
        id: "trial-balance",
        permission: permissions::REPORTS,
        title: "reports.trial_balance",
        href: "/accounting/reports/trial-balance",
    },
    // Bounded: it renders in the request and never becomes a row. It is here
    // for the same reason the other two are - the browser cannot write a file,
    // so even the immediate path draws on the server.
    ServerReport {
        id: "receipt",
        permission: permissions::PAYMENTS,
        title: "payments.receipt",
        href: "/selling/payments",
    },
];

/// Where a report with these parameters is read.
///
/// What the browser printing it opens. It is built from the request's own
/// parameters rather than from anything on a screen, so the file is of what
/// was asked for - a statement over the span in the row, not over whatever
/// span a page would have opened on.
///
/// `None` for a report whose address cannot be built from what the request
/// carries, which stops the export rather than printing the wrong page.
pub fn address(report_id: &str, parameters: &serde_json::Value) -> Option<String> {
    let text = |name: &str| {
        parameters
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };

    match report_id {
        "product-list" => Some("/inventory/items/report".to_owned()),
        "trial-balance" => {
            let from = text("from")?;
            let to = text("to")?;

            Some(format!(
                "/accounting/reports/trial-balance?from={from}&to={to}"
            ))
        }
        "customer-statement" => {
            let party = text("party_id")?;
            let from = text("from")?;
            let to = text("to")?;

            Some(format!(
                "/accounting/reports/statement/{party}?from={from}&to={to}"
            ))
        }
        "receipt" => Some(format!("/selling/payments/{}/receipt", text("payment_id")?)),
        _ => None,
    }
}

/// The report with this id, if the server can run it.
pub fn server_report(id: &str) -> Option<&'static ServerReport> {
    SERVER_REPORTS.iter().find(|report| report.id == id)
}

#[cfg(feature = "ssr")]
pub use render::render;

#[cfg(feature = "ssr")]
mod render {
    use app_books::report::CustomerStatement;
    use chrono::NaiveDate;
    use phonix_core::query::PageRequest;
    use phonix_core::report::Rendered;
    use phonix_db::sqlx::PgPool;
    use phonix_services::caller::Caller;
    use phonix_services::error::{ServiceError, ServiceResult};
    use serde_json::Value as Json;
    use uuid::Uuid;

    use crate::ui::report::ReportDefinition;
    use crate::ui::report::config::customer_statement::customer_statement;
    use crate::ui::report::config::product_list::{ROWS_PER_RUN, product_list};
    use crate::ui::report::config::receipt::receipt;
    use crate::ui::report::config::trial_balance::trial_balance;

    /// Draw one report on the server, as `caller`.
    ///
    /// Every arm reads through the service the screen reads through, so an
    /// export cannot see rows the screen would refuse: the permission on the
    /// entry gates the request, and the read gates itself again.
    pub async fn render(
        pool: &PgPool,
        caller: &Caller,
        report_id: &str,
        parameters: &Json,
    ) -> ServiceResult<Rendered> {
        match report_id {
            "product-list" => {
                let page = phonix_services::inventory::item::list(
                    pool,
                    caller,
                    PageRequest {
                        page: 1,
                        per_page: ROWS_PER_RUN,
                        ..PageRequest::default()
                    },
                )
                .await?;

                Ok(dressed(pool, &product_list(), &page).await)
            }
            "trial-balance" => {
                let from = date(parameters, "from")?;
                let to = date(parameters, "to")?;
                let report =
                    phonix_services::books::report::trial_balance(pool, caller, from, to).await?;

                Ok(dressed(pool, &trial_balance(), &report).await)
            }
            "customer-statement" => {
                let statement = statement(pool, caller, parameters).await?;

                Ok(dressed(pool, &customer_statement(), &statement).await)
            }
            "receipt" => {
                let payment_id = id_of(parameters, "payment_id")?;
                let payment =
                    phonix_services::books::payment::find(pool, caller, payment_id).await?;

                Ok(dressed(pool, &receipt(), &payment).await)
            }
            // A report the server does not know how to draw. Not a failure of
            // this request so much as a definition that was never added here.
            _ => Err(ServiceError::NotFound("report")),
        }
    }

    /// A report in the look the workspace keeps for its kind.
    ///
    /// The screen resolves the same settings out of a context; a worker has
    /// no context, so it reads them here. A report that is not one of the
    /// workspace's documents - a product list - keeps what its definition
    /// says, and a setting that cannot be read is not worth failing an export
    /// over.
    async fn dressed<T: Send + Sync + 'static>(
        pool: &PgPool,
        definition: &ReportDefinition<T>,
        data: &T,
    ) -> Rendered {
        let mut rendered = definition.rendered(data);

        if let Some(document) = definition.document()
            && let Ok(settings) =
                phonix_services::workspace::documents::current(pool, document).await
        {
            rendered.theme = settings.theme;
            rendered.page = settings.page();
        }

        rendered
    }

    /// The statement the parameters name.
    ///
    /// A parameter that is missing or unreadable is a refusal rather than a
    /// default: an export of "some customer, some span" is a document nobody
    /// asked for.
    async fn statement(
        pool: &PgPool,
        caller: &Caller,
        parameters: &Json,
    ) -> ServiceResult<CustomerStatement> {
        let party_id = id_of(parameters, "party_id")?;

        let from = date(parameters, "from")?;
        let to = date(parameters, "to")?;

        phonix_services::books::report::customer_statement(pool, caller, party_id, from, to).await
    }

    /// One id out of the parameters, or a refusal.
    ///
    /// A missing parameter is not a default: an export of "some customer" is a
    /// document nobody asked for.
    fn id_of(parameters: &Json, name: &'static str) -> ServiceResult<Uuid> {
        parameters
            .get(name)
            .and_then(Json::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .ok_or(ServiceError::NotFound("report parameter"))
    }

    fn date(parameters: &Json, name: &'static str) -> ServiceResult<NaiveDate> {
        parameters
            .get(name)
            .and_then(Json::as_str)
            .and_then(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
            .ok_or(ServiceError::NotFound("span"))
    }
}
