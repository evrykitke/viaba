//! One customer's account: whose accounts can be read, and the statement one
//! of them opens.
//!
//! The screen is the list, the span and the fetch; what the statement *is*
//! lives in
//! [`ui::report::config::customer_statement`](crate::ui::report::config::customer_statement).

use leptos::prelude::*;
use leptos_meta::Title;
use uuid::Uuid;

use crate::components::page::{PageHeader, Panel};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::customer_statement;
use crate::ui::report::config::customer_statement::customer_statement as definition;
use crate::ui::report::{Report, ReportViewer};
use crate::ui::table::DataGrid;
use crate::ui::table::config::statement_customers::statement_customers_grid;

use super::shared::SpanPicker;

/// Whose account can be read.
#[component]
pub fn customer_statement_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("reports.customer_statement")) />

        <PageHeader
            title=l!("reports.customer_statement")
            subtitle=l!("reports.statement.pick")
            icon=Icon::Receipt
            back=("/accounting", l!("nav.accounting"))
        />

        <DataGrid config=statement_customers_grid() />
    }
}

/// One customer's statement, in the viewer.
#[component]
pub fn customer_statement_report_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let party_id = move || params.with(|params| params.get("party").unwrap_or_default());

    // The address wins where it says anything. A span in the query is what
    // lets a report be linked to, and it is how the browser that prints one is
    // told which span to print - otherwise a printed statement would be of
    // whatever the screen opens on.
    let query = leptos_router::hooks::use_query_map();
    let asked = move || {
        query.with(|query| {
            let read = |name: &str| {
                query
                    .get(name)
                    .and_then(|raw| chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d").ok())
            };

            read("from").zip(read("to"))
        })
    };

    let span = super::shared::opening_span(asked());

    let statement = Resource::new(
        move || (party_id(), span.get()),
        |(party, span)| async move {
            match (party.parse::<Uuid>(), span) {
                (Ok(party_id), Some((from, to))) => {
                    customer_statement(party_id, from, to).await.ok()
                }
                _ => None,
            }
        },
    );

    view! {
        <Title text=format!("{} | Evrykit", l!("reports.customer_statement")) />

        <ReportViewer
            definition=definition()
            back=("/accounting/reports/statement", l!("reports.customer_statement"))
            controls=move || view! { <SpanPicker span=span /> }.into_any()
            rows=Signal::derive(move || {
                statement.get().flatten().map_or(0, |statement| statement.lines.len())
            })
            parameters=Signal::derive(move || {
                serde_json::json!({
                    "party_id": party_id(),
                    "from": span.get().map(|(from, _)| from.to_string()),
                    "to": span.get().map(|(_, to)| to.to_string()),
                })
            })
        >
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    match statement.await {
                        Some(statement) => {
                            view! { <Report definition=definition() data=statement /> }.into_any()
                        }
                        None => {
                            view! {
                                <Panel>
                                    <p class="py-6 text-center text-sm text-content-muted">
                                        {l!("reports.empty")}
                                    </p>
                                </Panel>
                            }
                                .into_any()
                        }
                    }
                })}
            </Transition>
        </ReportViewer>
    }
}
