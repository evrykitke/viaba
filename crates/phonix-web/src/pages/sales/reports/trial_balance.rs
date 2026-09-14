//! Every account, both columns, and the proof they agree.

use app_books::report::TrialBalance;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::{Notice, PageHeader, Panel, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::trial_balance;

use super::shared::{ReportNote, SpanPicker, MONEY_CELL, MONEY_HEAD, MONEY_TOTAL, amount};

#[component]
pub fn trial_balance_page() -> impl IntoView {
    let span = super::shared::opening_span();

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
        <Title text=format!("{} | Phonix", l!("reports.trial_balance")) />

        <PageHeader
            title=l!("reports.trial_balance")
            subtitle=l!("reports.trial_balance.subtitle")
            icon=Icon::Table
            back=("/sales", l!("nav.sales"))
        >
            <SpanPicker span=span />
        </PageHeader>

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                report
                    .await
                    .map(|report| view! { <Sheet report=report /> })
            })}
        </Transition>
    }
}

#[component]
fn sheet(report: TrialBalance) -> impl IntoView {
    let currency = report.currency.code().to_owned();
    let balanced = report.is_balanced();
    let empty = report.rows.is_empty();

    view! {
        <Panel>
            <div class="space-y-3">
                <ReportNote currency=currency />

                <Show when=move || !balanced fallback=|| ()>
                    <Notice
                        message=Signal::derive(|| Some(l!("reports.not_balanced")))
                        tone=Tone::Danger
                    />
                </Show>

                <Show when=move || empty fallback=|| ()>
                    <p class="py-6 text-center text-sm text-content-muted">{l!("reports.empty")}</p>
                </Show>

                <Show when=move || !empty fallback=|| ()>
                    <div class="overflow-x-auto">
                        <table class="w-full min-w-[48rem] text-sm">
                            <thead>
                                <tr class="border-b border-edge text-left text-xs text-content-muted">
                                    <th class="py-2 font-medium">{l!("reports.column.account")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.opening")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.debit")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.credit")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.closing_debit")}</th>
                                    <th class=MONEY_HEAD>{l!("reports.column.closing_credit")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {report
                                    .rows
                                    .iter()
                                    .map(|row| {
                                        view! {
                                            <tr class="border-b border-edge/60">
                                                <td class="py-1.5 text-content">
                                                    <span class="font-mono text-xs text-content-subtle">
                                                        {row.number.clone()}
                                                    </span>
                                                    " "
                                                    {row.name.clone()}
                                                </td>
                                                <td class=MONEY_CELL>{amount(row.opening)}</td>
                                                <td class=MONEY_CELL>{amount(row.debits)}</td>
                                                <td class=MONEY_CELL>{amount(row.credits)}</td>
                                                <td class=MONEY_TOTAL>{amount(row.closing_debit)}</td>
                                                <td class=MONEY_TOTAL>{amount(row.closing_credit)}</td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()}
                            </tbody>
                            <tfoot>
                                <tr class="border-t-2 border-edge">
                                    <td class="py-2 text-sm font-medium text-content">
                                        {l!("reports.total")}
                                    </td>
                                    <td class=MONEY_CELL></td>
                                    <td class=MONEY_TOTAL>{amount(report.debits)}</td>
                                    <td class=MONEY_TOTAL>{amount(report.credits)}</td>
                                    <td class=MONEY_TOTAL>{amount(report.closing_debits)}</td>
                                    <td class=MONEY_TOTAL>{amount(report.closing_credits)}</td>
                                </tr>
                            </tfoot>
                        </table>
                    </div>

                    <Show when=move || balanced fallback=|| ()>
                        <p class="text-xs text-content-subtle">{l!("reports.balanced")}</p>
                    </Show>
                </Show>
            </div>
        </Panel>
    }
}
