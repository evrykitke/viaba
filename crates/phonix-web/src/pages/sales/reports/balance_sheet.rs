//! What is owned and what is owed, at one date.
//!
//! The screen is the fetch; what the statement *is* lives in
//! [`ui::report::config::balance_sheet`](crate::ui::report::config::balance_sheet).

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::Panel;
use crate::l;
use crate::server_fns::books_fns::balance_sheet;
use crate::ui::report::config::balance_sheet::balance_sheet as definition;
use crate::ui::report::{Report, ReportViewer};

use super::shared::AsAtPicker;

#[component]
pub fn balance_sheet_page() -> impl IntoView {
    let query = leptos_router::hooks::use_query_map();
    let asked = move || {
        query.with(|query| {
            query
                .get("as_at")
                .and_then(|raw| chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d").ok())
        })
    };

    let span = super::shared::opening_span(asked().map(|as_at| (as_at, as_at)));

    // A balance sheet is read at a date rather than over a span: a photograph,
    // not a period.
    let as_at = Signal::derive(move || span.get().map(|(_, to)| to));

    let report = Resource::new(
        move || as_at.get(),
        |as_at| async move {
            match as_at {
                Some(as_at) => balance_sheet(as_at).await.ok(),
                None => None,
            }
        },
    );

    view! {
        <Title text=format!("{} | Phonix", l!("reports.balance_sheet")) />

        <ReportViewer
            definition=definition()
            back=("/accounting", l!("nav.accounting"))
            controls=move || view! { <AsAtPicker span=span /> }.into_any()
            parameters=Signal::derive(move || {
                serde_json::json!({ "as_at": as_at.get().map(|at| at.to_string()) })
            })
            rows=Signal::derive(move || {
                report
                    .get()
                    .flatten()
                    .map_or(
                        0,
                        |report| {
                            report.assets.lines.len() + report.liabilities.lines.len()
                                + report.equity.lines.len() + 2
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
