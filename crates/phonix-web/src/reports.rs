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

use phonix_core::permissions;

/// One report the server can run.
pub struct ServerReport {
    /// The definition's own id.
    pub id: &'static str,
    /// What somebody must hold to run it.
    pub permission: &'static str,
}

/// Every report that can be run away from a browser.
///
/// A report missing from this list can still be read on screen; what it cannot
/// be is exported, because nothing here knows how to draw it without one.
pub const SERVER_REPORTS: &[ServerReport] = &[
    ServerReport {
        id: "product-list",
        permission: permissions::ITEMS,
    },
    ServerReport {
        id: "customer-statement",
        permission: permissions::REPORTS,
    },
];

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

    use crate::ui::report::config::customer_statement::customer_statement;
    use crate::ui::report::config::product_list::{ROWS_PER_RUN, product_list};

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

                Ok(product_list().rendered(&page))
            }
            "customer-statement" => {
                let statement = statement(pool, caller, parameters).await?;

                Ok(customer_statement().rendered(&statement))
            }
            // A report the server does not know how to draw. Not a failure of
            // this request so much as a definition that was never added here.
            _ => Err(ServiceError::NotFound("report")),
        }
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
        let party_id = parameters
            .get("party_id")
            .and_then(Json::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .ok_or(ServiceError::NotFound("customer"))?;

        let from = date(parameters, "from")?;
        let to = date(parameters, "to")?;

        phonix_services::books::report::customer_statement(pool, caller, party_id, from, to).await
    }

    fn date(parameters: &Json, name: &'static str) -> ServiceResult<NaiveDate> {
        parameters
            .get(name)
            .and_then(Json::as_str)
            .and_then(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
            .ok_or(ServiceError::NotFound("span"))
    }
}
