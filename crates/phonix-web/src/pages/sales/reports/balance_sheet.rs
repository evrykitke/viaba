//! What is owned and what is owed, at one date.

use app_books::report::{BalanceSheet, ReportGroup};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::money::Money;

use crate::components::page::{Notice, PageHeader, Panel, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::balance_sheet;

use super::shared::{AsAtPicker, MONEY_CELL, MONEY_TOTAL, ReportNote, amount};

#[component]
pub fn balance_sheet_page() -> impl IntoView {
    let span = super::shared::opening_span();

    // Only the far end of the span matters here: a balance sheet is a
    // photograph, not a period.
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

        <PageHeader
            title=l!("reports.balance_sheet")
            subtitle=l!("reports.balance_sheet.subtitle")
            icon=Icon::ClipboardList
            back=("/accounting", l!("nav.accounting"))
        >
            <AsAtPicker span=span />
        </PageHeader>

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                report.await.map(|report| view! { <Sheet report=report /> })
            })}
        </Transition>
    }
}

#[component]
fn sheet(report: BalanceSheet) -> impl IntoView {
    let currency = report.currency.code().to_owned();
    let balanced = report.is_balanced();
    let note = l!(
        "reports.balance_sheet.note",
        opened = report.year_opened.to_string()
    );

    let empty = report.assets.is_empty()
        && report.liabilities.is_empty()
        && report.equity.is_empty()
        && report.brought_forward.is_zero()
        && report.result_for_year.is_zero();

    view! {
        <div class="max-w-3xl">
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
                        <p class="py-6 text-center text-sm text-content-muted">
                            {l!("reports.empty")}
                        </p>
                    </Show>

                    <Show when=move || !empty fallback=|| ()>
                        <table class="w-full text-sm">
                            <tbody>
                                <Group
                                    title=l!("reports.section.assets")
                                    group=report.assets.clone()
                                />
                                <Total
                                    label=l!("reports.total_assets")
                                    amount=report.total_assets
                                    strong=true
                                />
                                <Group
                                    title=l!("reports.section.liabilities")
                                    group=report.liabilities.clone()
                                />
                                <Group
                                    title=l!("reports.section.equity")
                                    group=report.equity.clone()
                                    always=true
                                />
                                <Line
                                    label=l!("reports.brought_forward")
                                    amount=report.brought_forward
                                />
                                <Line
                                    label=l!("reports.result_for_year")
                                    amount=report.result_for_year
                                />
                                <Total
                                    label=l!("reports.total_funding")
                                    amount=report.total_funding
                                    strong=true
                                />
                            </tbody>
                        </table>

                        <p class="text-xs text-content-subtle">
                            {note.clone()}
                        </p>
                    </Show>
                </div>
            </Panel>
        </div>
    }
}

/// One section of the sheet.
///
/// `always` draws the heading even with nothing under it, which equity needs:
/// the two earnings lines belong inside that section and are printed by the
/// caller, so a workspace with no equity accounts of its own still has an
/// equity section.
#[component]
fn group(
    #[prop(into)] title: String,
    group: ReportGroup,
    #[prop(optional)] always: bool,
) -> impl IntoView {
    if group.is_empty() && !always {
        return ().into_any();
    }

    let total = group.total;
    let lines = group.lines;
    let has_lines = !lines.is_empty();

    view! {
        <>
            <tr>
                <td class="pt-4 pb-1 text-xs font-medium uppercase tracking-wide text-content-muted">
                    {title}
                </td>
                <td></td>
            </tr>
            {lines
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
            {has_lines
                .then(|| {
                    view! {
                        <tr class="border-b border-edge">
                            <td class="py-1.5 text-sm text-content-muted">{l!("reports.total")}</td>
                            <td class=MONEY_TOTAL>{amount(total)}</td>
                        </tr>
                    }
                })}
        </>
    }
        .into_any()
}

/// A line that belongs to a section but not to an account.
#[component]
fn line(#[prop(into)] label: String, amount: Money) -> impl IntoView {
    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1.5 text-content">{label}</td>
            <td class=MONEY_CELL>{super::shared::amount(amount)}</td>
        </tr>
    }
}

/// One of the two figures that have to agree.
#[component]
fn total(
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
