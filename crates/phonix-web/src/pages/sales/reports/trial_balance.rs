//! Every account, both columns, and the proof they agree.
//!
//! The screen is the fetch; what the statement *is* lives in
//! [`ui::report::config::trial_balance`](crate::ui::report::config::trial_balance).

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::Panel;
use crate::l;
use crate::server_fns::books_fns::trial_balance;
use crate::ui::report::config::trial_balance::trial_balance as definition;
use crate::ui::report::{Report, ReportViewer};

use super::shared::SpanPicker;

#[component]
pub fn trial_balance_page() -> impl IntoView {
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
                Some((from, to)) => trial_balance(from, to).await.ok(),
                None => None,
            }
        },
    );

    view! {
        <Title text=format!("{} | Evrykit", l!("reports.trial_balance")) />

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
                report.get().flatten().map_or(0, |report| report.rows.len())
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
