//! One customer's account: what they were invoiced, what they have paid, and
//! how long the rest has been owed.

use app_books::report::CustomerStatement;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::money::Money;
use uuid::Uuid;

use crate::components::page::{PageHeader, Panel};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::{customer_statement, statement_customers};

use super::shared::{MONEY_CELL, MONEY_HEAD, MONEY_TOTAL, ReportNote, SpanPicker, amount};

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

        <PageHeader
            title=l!("reports.customer_statement")
            subtitle=l!("reports.customer_statement.subtitle")
            icon=Icon::Receipt
            back=("/accounting", l!("nav.accounting"))
        >
            <div class="flex flex-wrap items-center gap-3">
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
            </div>
        </PageHeader>

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match statement.await {
                    Some(statement) => view! { <Statement statement=statement /> }.into_any(),
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
    }
}

#[component]
fn statement(statement: CustomerStatement) -> impl IntoView {
    let currency = statement.currency.code().to_owned();
    let party = format!("{} \u{b7} {}", statement.party_code, statement.party_name);
    let empty = statement.lines.is_empty();
    let ageing = statement.ageing.clone();

    view! {
        <div class="space-y-3">
            <Panel title=party>
                <div class="space-y-3">
                    <ReportNote currency=currency />

                    <div class="overflow-x-auto">
                        <table class="w-full min-w-[40rem] text-sm">
                            <thead>
                                <tr class="border-b border-edge text-left text-xs text-content-muted">
                                    <th class="py-2 font-medium">{l!("reports.column.date")}</th>
                                    <th class="py-2 font-medium">{l!("reports.column.document")}</th>
                                    <th class="py-2 font-medium">{l!("field.type")}</th>
                                    <th class="py-2 font-medium">{l!("reports.column.due")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.amount")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.balance")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr class="border-b border-edge/60">
                                    <td class="py-1.5 text-content-muted" colspan="4">
                                        {l!("reports.statement.opening")}
                                    </td>
                                    <td class=MONEY_CELL></td>
                                    <td class=MONEY_TOTAL>{amount(statement.opening)}</td>
                                </tr>

                                {statement
                                    .lines
                                    .iter()
                                    .map(|line| {
                                        // What the document says, where that is not
                                        // what it is worth in the books.
                                        let foreign = (line.document.currency()
                                            != statement.currency)
                                            .then(|| line.document.to_string());
                                        let kind = crate::i18n::t(&line.kind.label());

                                        view! {
                                            <tr class="border-b border-edge/60">
                                                <td class="py-1.5 text-content-muted">
                                                    {line.dated_on.to_string()}
                                                </td>
                                                <td class="py-1.5 text-content">
                                                    {line.number.clone()}
                                                    {foreign
                                                        .map(|text| {
                                                            view! {
                                                                <div class="text-2xs text-content-subtle">{text}</div>
                                                            }
                                                        })}
                                                </td>
                                                <td class="py-1.5 text-xs text-content-subtle">
                                                    {kind}
                                                </td>
                                                <td class="py-1.5 text-content-muted">
                                                    {line
                                                        .due_on
                                                        .map(|due| due.to_string())
                                                        .unwrap_or_default()}
                                                </td>
                                                <td class=MONEY_CELL>{amount(line.amount)}</td>
                                                <td class=MONEY_TOTAL>{amount(line.running)}</td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()}

                                <Show when=move || empty fallback=|| ()>
                                    <tr>
                                        <td
                                            class="py-6 text-center text-sm text-content-muted"
                                            colspan="6"
                                        >
                                            {l!("reports.empty")}
                                        </td>
                                    </tr>
                                </Show>
                            </tbody>
                            <tfoot>
                                <tr class="border-t border-edge">
                                    <td class="py-1.5 text-sm text-content-muted" colspan="4">
                                        {l!("reports.statement.billed")}
                                    </td>
                                    <td class=MONEY_CELL></td>
                                    <td class=MONEY_TOTAL>{amount(statement.billed)}</td>
                                </tr>
                                <tr>
                                    <td class="py-1.5 text-sm text-content-muted" colspan="4">
                                        {l!("reports.statement.received")}
                                    </td>
                                    <td class=MONEY_CELL></td>
                                    <td class=MONEY_TOTAL>{amount(statement.received)}</td>
                                </tr>
                                <tr class="border-t-2 border-edge">
                                    <td class="py-2 text-sm font-medium text-content" colspan="4">
                                        {l!("reports.statement.closing")}
                                    </td>
                                    <td class=MONEY_CELL></td>
                                    <td class="py-2 pl-3 text-right font-medium tabular-nums text-content">
                                        {amount(statement.closing)}
                                    </td>
                                </tr>
                            </tfoot>
                        </table>
                    </div>
                </div>
            </Panel>

            <Panel title=l!("reports.ageing") description=l!("reports.ageing.help")>
                <div class="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
                    <Bucket label=l!("reports.ageing.not_due") amount=ageing.not_yet_due />
                    <Bucket label=l!("reports.ageing.to_30") amount=ageing.to_30 />
                    <Bucket label=l!("reports.ageing.to_60") amount=ageing.to_60 />
                    <Bucket label=l!("reports.ageing.to_90") amount=ageing.to_90 />
                    <Bucket label=l!("reports.ageing.over_90") amount=ageing.over_90 />
                    // Not a rung: a credit belonging to no invoice, subtracted
                    // from the five rather than sitting among them.
                    <Bucket label=l!("reports.ageing.on_account") amount=ageing.on_account />
                </div>
            </Panel>
        </div>
    }
}

/// One rung of the ladder.
#[component]
fn bucket(#[prop(into)] label: String, amount: Money) -> impl IntoView {
    view! {
        <div class="rounded-control border border-edge px-3 py-2">
            <p class="text-xs text-content-subtle">{label}</p>
            <p class="text-sm font-medium tabular-nums text-content">
                {super::shared::amount(amount)}
            </p>
        </div>
    }
}
