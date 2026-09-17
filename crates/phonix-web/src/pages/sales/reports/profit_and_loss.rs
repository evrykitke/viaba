//! What was earned and what it cost, between two dates.
//!
//! The screen is the fetch; what the statement *is* lives in
//! [`ui::report::config::profit_and_loss`](crate::ui::report::config::profit_and_loss).

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::Panel;
use crate::l;
use crate::server_fns::books_fns::profit_and_loss;
use crate::ui::report::config::profit_and_loss::profit_and_loss as definition;
use crate::ui::report::{Report, ReportViewer};

use super::shared::SpanPicker;

#[component]
pub fn profit_and_loss_page() -> impl IntoView {
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

    let report = Resource::new(
        move || span.get(),
        |span| async move {
            match span {
                Some((from, to)) => profit_and_loss(from, to).await.ok(),
                None => None,
            }
        },
    );

    view! {
        <Title text=format!("{} | Phonix", l!("reports.profit_and_loss")) />

        <ReportViewer
            definition=definition()
            back=("/accounting", l!("nav.accounting"))
            controls=move || view! { <SpanPicker span=span /> }.into_any()
            parameters=Signal::derive(move || {
                serde_json::json!({
                    "from": span.get().map(|(from, _)| from.to_string()),
                    "to": span.get().map(|(_, to)| to.to_string()),
                })
            })
            rows=Signal::derive(move || {
                report
                    .get()
                    .flatten()
                    .map_or(
                        0,
                        |report| {
                            report.revenue.lines.len() + report.cost_of_sales.lines.len()
                                + report.operating_expenses.lines.len()
                                + report.other_income.lines.len()
                                + report.other_expenses.lines.len()
                        },
                    )
            })
        >
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    match report.await {
                        Some(report) => {
                            view! { <Report definition=definition() data=report /> }.into_any()
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
