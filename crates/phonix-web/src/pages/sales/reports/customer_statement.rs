//! One customer's account, opened in the report viewer.
//!
//! The screen is the pickers and the fetch; what the statement *is* lives in
//! [`ui::report::config::customer_statement`](crate::ui::report::config::customer_statement).

use leptos::prelude::*;
use leptos_meta::Title;
use uuid::Uuid;

use crate::components::page::Panel;
use crate::l;
use crate::server_fns::books_fns::{customer_statement, statement_customers};
use crate::ui::report::config::customer_statement::customer_statement as definition;
use crate::ui::report::{Report, ReportViewer};

use super::shared::SpanPicker;

#[component]
pub fn customer_statement_page() -> impl IntoView {
    let span = super::shared::opening_span();
    let customer = RwSignal::new(None::<Uuid>);

    let customers = Resource::new(
        || (),
        |()| async move { statement_customers().await.unwrap_or_default() },
    );

    let statement = Resource::new(
        move || (customer.get(), span.get()),
        |(customer, span)| async move {
            match (customer, span) {
                (Some(party_id), Some((from, to))) => {
                    customer_statement(party_id, from, to).await.ok()
                }
                _ => None,
            }
        },
    );

    view! {
        <Title text=format!("{} | Phonix", l!("reports.customer_statement")) />

        <ReportViewer
            definition=definition()
            back=("/accounting", l!("nav.accounting"))
            controls=move || {
                view! {
                    <Transition fallback=|| ()>
                        {move || Suspend::new(async move {
                            let customers = customers.await;

                            view! {
                                <label class="flex items-center gap-2 text-xs text-content-subtle">
                                    {l!("reports.customer")}
                                    <select
                                        class="h-8 rounded-control border border-edge bg-surface px-2 text-sm text-content"
                                        on:change=move |ev| {
                                            customer.set(event_target_value(&ev).parse().ok());
                                        }
                                    >
                                        <option value="">{l!("reports.statement.pick")}</option>
                                        {customers
                                            .into_iter()
                                            .map(|party| {
                                                let id = party.id.to_string();
                                                let text = format!(
                                                    "{} \u{b7} {}",
                                                    party.code,
                                                    party.name,
                                                );

                                                view! { <option value=id>{text}</option> }
                                            })
                                            .collect_view()}
                                    </select>
                                </label>
                            }
                        })}
                    </Transition>

                    <SpanPicker span=span />
                }
                    .into_any()
            }
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
                                        {l!("reports.statement.pick")}
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
