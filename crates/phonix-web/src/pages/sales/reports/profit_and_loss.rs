//! What was earned and what it cost, between two dates.

use app_books::report::{IncomeStatement, ReportGroup};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::money::Money;

use crate::components::page::{PageHeader, Panel};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::profit_and_loss;

use super::shared::{MONEY_CELL, MONEY_TOTAL, ReportNote, SpanPicker, amount};

#[component]
pub fn profit_and_loss_page() -> impl IntoView {
    let span = super::shared::opening_span();

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

        <PageHeader
            title=l!("reports.profit_and_loss")
            subtitle=l!("reports.profit_and_loss.subtitle")
            icon=Icon::ChartColumn
            back=("/accounting", l!("nav.accounting"))
        >
            <SpanPicker span=span />
        </PageHeader>

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                report.await.map(|report| view! { <Statement report=report /> })
            })}
        </Transition>
    }
}

#[component]
fn statement(report: IncomeStatement) -> impl IntoView {
    let currency = report.currency.code().to_owned();

    // Nothing earned and nothing spent. Said once rather than as five empty
    // sections and four zeroes.
    let empty = report.revenue.is_empty()
        && report.cost_of_sales.is_empty()
        && report.operating_expenses.is_empty()
        && report.other_income.is_empty()
        && report.other_expenses.is_empty();

    view! {
        <div class="max-w-3xl">
            <Panel>
                <div class="space-y-3">
                    <ReportNote currency=currency />

                    <Show when=move || empty fallback=|| ()>
                        <p class="py-6 text-center text-sm text-content-muted">
                            {l!("reports.empty")}
                        </p>
                    </Show>

                    <Show when=move || !empty fallback=|| ()>
                        <table class="w-full text-sm">
                            <tbody>
                                <Group
                                    title=l!("reports.section.revenue")
                                    group=report.revenue.clone()
                                />
                                <Group
                                    title=l!("reports.section.cost_of_sales")
                                    group=report.cost_of_sales.clone()
                                />
                                <Subtotal
                                    label=l!("reports.gross_profit")
                                    amount=report.gross_profit
                                />
                                <Group
                                    title=l!("reports.section.operating_expenses")
                                    group=report.operating_expenses.clone()
                                />
                                <Subtotal
                                    label=l!("reports.operating_profit")
                                    amount=report.operating_profit
                                />
                                <Group
                                    title=l!("reports.section.other_income")
                                    group=report.other_income.clone()
                                />
                                <Group
                                    title=l!("reports.section.other_expenses")
                                    group=report.other_expenses.clone()
                                />
                                <Subtotal
                                    label=l!("reports.net_profit")
                                    amount=report.net_profit
                                    strong=true
                                />
                            </tbody>
                        </table>
                    </Show>
                </div>
            </Panel>
        </div>
    }
}

/// One section: a heading, its accounts, and what they come to.
///
/// Draws nothing at all when the section is empty. A profit and loss with
/// five headings and two lines under them reads as a form somebody failed to
/// fill in.
#[component]
fn group(#[prop(into)] title: String, group: ReportGroup) -> impl IntoView {
    if group.is_empty() {
        return ().into_any();
    }

    let total = group.total;

    view! {
        <>
            <tr>
                <td class="pt-4 pb-1 text-xs font-medium uppercase tracking-wide text-content-muted">
                    {title}
                </td>
                <td></td>
            </tr>
            {group
                .lines
                .into_iter()
                .map(|line| {
                    view! {
                        <tr class="border-b border-edge/60">
                            <td class="py-1.5 text-content">
                                <span class="font-mono text-xs text-content-subtle">
                                    {line.number}
                                </span>
                                " "
                                {line.name}
                            </td>
                            <td class=MONEY_CELL>{amount(line.amount)}</td>
                        </tr>
                    }
                })
                .collect_view()}
            <tr class="border-b border-edge">
                <td class="py-1.5 text-sm text-content-muted">{l!("reports.total")}</td>
                <td class=MONEY_TOTAL>{amount(total)}</td>
            </tr>
        </>
    }
        .into_any()
}

/// A figure somebody came to read.
#[component]
fn subtotal(
    #[prop(into)] label: String,
    amount: Money,
    #[prop(optional)] strong: bool,
) -> impl IntoView {
    let row = if strong {
        "border-y-2 border-edge bg-surface-raised/60"
    } else {
        "border-y border-edge"
    };

    view! {
        <tr class=row>
            <td class="py-2 text-sm font-medium text-content">{label}</td>
            <td class="py-2 pl-3 text-right font-medium tabular-nums text-content">
                {super::shared::amount(amount)}
            </td>
        </tr>
    }
}
