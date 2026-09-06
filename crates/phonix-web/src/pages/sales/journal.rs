//! One journal, as a document.
//!
//! Read-only, always, and that is the screen's whole argument. There is no edit
//! mode to slip into: a posted journal is evidence, and the only thing that can
//! be done to it is to reverse it - which raises a second journal rather than
//! changing this one.
//!
//! The two columns are debit and credit, not one signed amount. That is how an
//! accountant reads a journal, and it is why the store keeps a side and a
//! magnitude instead of a sign.

use app_books::account::Side;
use app_books::journal::{Posted, PostedLine};
use chrono::Utc;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::money::Money;
use phonix_core::permissions;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::server_fns::books_fns::{journal_detail, reverse_journal};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::tabs::{Tab, TabbedPanel};
use crate::ui::viewer::Viewer;

#[component]
pub fn journal_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let journal_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let reload = RwSignal::new(0_u32);
    let journal = Resource::new(
        move || (journal_id(), reload.get()),
        |(raw, _)| async move {
            match raw.parse::<Uuid>() {
                Ok(id) => journal_detail(id).await,
                Err(_) => Err(ServerFnError::new("That is not a journal id.")),
            }
        },
    );

    view! {
        <Title text=format!("{} | Phonix", l!("entity.journal.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match journal.await {
                    Ok(journal) => {
                        view! {
                            <JournalDocument
                                journal=journal
                                reload=Callback::new(move |()| {
                                    reload.update(|count| *count += 1)
                                })
                            />
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.journal.singular")
                                    icon=Icon::ScrollText
                                    back=("/sales/journals", l!("journals.title"))
                                />
                                <Notice
                                    message=Signal::derive(move || Some(err.to_string()))
                                    tone=Tone::Danger
                                />
                            </>
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
    }
}

#[component]
fn journal_document(journal: Posted, reload: Callback<()>) -> impl IntoView {
    let journal_id = journal.id;
    let number = journal.number.clone();
    let is_reversal = journal.reverses_id.is_some();
    let reverses_id = journal.reverses_id;

    let header = journal.clone();
    let lines = journal.lines.clone();

    let details_tab = Tab::new("details", "Details", move || {
        let header = header.clone();
        let lines = lines.clone();

        view! {
            <div class="space-y-4">
                <JournalFacts journal=header />
                <LineTable lines=lines />
            </div>
        }
        .into_any()
    })
    .icon(Icon::ScrollText);

    let history_tab = Tab::new("history", "History", move || {
        view! { <RecordHistory kind=kinds::JOURNAL id=Some(journal_id.to_string()) /> }.into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader
            title=number.clone()
            subtitle=journal.narration.clone()
            icon=Icon::ScrollText
            back=("/sales/journals", l!("journals.title"))
        >
            <div class="flex flex-wrap items-center gap-1.5">
                {is_reversal.then(|| view! { <Badge label=l!("journals.reversal") tone=Tone::Warning /> })}
                <ReverseButton journal_id=journal_id number=number.clone() reload=reload />
            </div>
        </PageHeader>

        {reverses_id
            .map(|original| {
                view! {
                    <div class="rounded-card border border-edge bg-surface-sunken px-4 py-2.5 text-sm text-content-muted">
                        {l!("journals.reverses")}
                        " "
                        <a
                            href=format!("/sales/journals/{original}")
                            class="text-brand hover:underline"
                        >
                            {l!("journals.the_original")}
                        </a>
                    </div>
                }
            })}

        <TabbedPanel id="journal" tabs=vec![details_tab, history_tab] />
    }
}

/// When it belongs to, where it came from, and what it moves.
#[component]
fn journal_facts(journal: Posted) -> impl IntoView {
    let total = side_total(&journal.lines, Side::Debit);

    view! {
        <dl class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <Fact label=l!("journals.entry_date") value=journal.entry_date.to_string() />
            <Fact label=l!("journals.period") value=journal.period_label.clone() />
            <Fact label=l!("journals.source") value=source_of(&journal) />
            <Fact label=l!("journals.total") value=total.to_display_string() />
        </dl>
    }
}

#[component]
fn fact(label: String, value: String) -> impl IntoView {
    view! {
        <div class="rounded-card border border-edge bg-surface-raised px-4 py-3">
            <dt class="text-xs text-content-subtle">{label}</dt>
            <dd class="mt-1 truncate-fade text-sm font-medium tabular-nums text-content">
                {value}
            </dd>
        </div>
    }
}

/// The journal itself: two money columns, and the totals that agree.
#[component]
fn line_table(lines: Vec<PostedLine>) -> impl IntoView {
    let debits = side_total(&lines, Side::Debit);
    let credits = side_total(&lines, Side::Credit);

    view! {
        <div class="overflow-x-auto rounded-card border border-edge bg-surface-raised">
            <table class="w-full min-w-[42rem] text-sm">
                <thead class="border-b border-edge text-xs text-content-subtle">
                    <tr>
                        <th class="px-4 py-2 text-left font-medium">{l!("field.account")}</th>
                        <th class="px-4 py-2 text-left font-medium">{l!("journals.memo")}</th>
                        <th class="px-4 py-2 text-right font-medium">{l!("books.side.debit")}</th>
                        <th class="px-4 py-2 text-right font-medium">{l!("books.side.credit")}</th>
                    </tr>
                </thead>
                <tbody>
                    {lines
                        .into_iter()
                        .map(|line| view! { <LineRow line=line /> })
                        .collect::<Vec<_>>()}
                </tbody>
                // Shown even though they are equal by construction. An
                // accountant checks the foot of a journal, and a ledger that
                // asks to be taken on trust is one nobody takes on trust.
                <tfoot class="border-t border-edge font-medium">
                    <tr>
                        <td class="px-4 py-2 text-xs text-content-subtle" colspan="2">
                            {l!("journals.totals")}
                        </td>
                        <td class="px-4 py-2 text-right tabular-nums">
                            {debits.to_display_string()}
                        </td>
                        <td class="px-4 py-2 text-right tabular-nums">
                            {credits.to_display_string()}
                        </td>
                    </tr>
                </tfoot>
            </table>
        </div>
    }
}

#[component]
fn line_row(line: PostedLine) -> impl IntoView {
    let is_debit = line.side == Side::Debit;
    let amount = line.base_amount.to_display_string();
    let account = format!("{} · {}", line.account_number, line.account_name);
    let account_href = format!("/sales/accounts/{}", line.account_id);
    let memo = line.memo.clone().unwrap_or_default();
    let dimensions = line.dimensions.clone();

    // Shown only where it differs. A rate of one on every line of a
    // single-currency ledger is a column of noise.
    let foreign = (line.amount.currency() != line.base_amount.currency())
        .then(|| format!("{} @ {}", line.amount.to_display_string(), line.exchange_rate));

    view! {
        <tr class="border-b border-edge last:border-b-0">
            <td class="px-4 py-2">
                <a href=account_href class="text-content hover:text-brand hover:underline">
                    {account}
                </a>
                {(!dimensions.is_empty())
                    .then(|| {
                        view! {
                            <div class="mt-0.5 flex flex-wrap gap-1">
                                {dimensions
                                    .into_iter()
                                    .map(|value| {
                                        view! {
                                            <span class="inline-flex items-center gap-1 rounded-full bg-surface-sunken px-2 py-0.5 text-2xs text-content-muted">
                                                <Icon icon=Icon::Building2 size=IconSize::Xs />
                                                {value.label()}
                                            </span>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                            </div>
                        }
                    })}
            </td>
            <td class="px-4 py-2 text-xs text-content-muted">
                {memo}
                {foreign
                    .map(|foreign| {
                        view! {
                            <div class="text-2xs text-content-subtle tabular-nums">{foreign}</div>
                        }
                    })}
            </td>
            <td class="px-4 py-2 text-right tabular-nums">
                {is_debit.then(|| amount.clone())}
            </td>
            <td class="px-4 py-2 text-right tabular-nums">{(!is_debit).then_some(amount)}</td>
        </tr>
    }
}

/// Reversing, which asks first.
///
/// A confirmation rather than a plain button because this posts a second
/// journal that cannot itself be undone except by a third. The dialog names
/// the journal so somebody who arrived from a list is sure which one they have
/// open.
#[component]
fn reverse_button(journal_id: Uuid, number: String, reload: Callback<()>) -> impl IntoView {
    let viewer = Viewer::get();
    let alerts = Alerts::get();

    let may_reverse = move || {
        viewer
            .get()
            .is_some_and(|user| user.can(permissions::JOURNALS_REVERSE))
    };

    let ask = move |_| {
        let number = number.clone();
        let question = l!("journals.reverse.confirm", number = number);

        alerts.ask(
            Confirm::new(question, move || {
                leptos::task::spawn_local(async move {
                    // Dated today, not on the original's date: a correction
                    // found in April is April's event, and March may be closed.
                    match reverse_journal(journal_id, Utc::now().date_naive(), None).await {
                        Ok(posted) => {
                            alerts.post(Alert::success(l!(
                                "journals.reversed",
                                number = posted.number
                            )));
                            reload.run(());
                        }
                        Err(err) => alerts.post(Alert::failure(err.to_string())),
                    }
                });
            })
            .titled(l!("journals.reverse"))
            .confirm_label(l!("journals.reverse")),
        );
    };

    view! {
        <Show when=may_reverse fallback=|| ()>
            <button
                type="button"
                class="flex h-8 items-center gap-1.5 rounded-control border border-edge px-2.5 text-sm text-content-muted hover:bg-surface-hover hover:text-content"
                on:click=ask.clone()
            >
                <Icon icon=Icon::Undo2 size=IconSize::Sm />
                {l!("journals.reverse")}
            </button>
        </Show>
    }
}

/// One side's total. Both sides agree by construction; this is what the foot of
/// the table shows so a reader can see that for themselves.
fn side_total(lines: &[PostedLine], side: Side) -> Money {
    let currency = lines
        .first()
        .map_or_else(Default::default, |line| line.base_amount.currency());

    lines
        .iter()
        .filter(|line| line.side == side)
        .try_fold(Money::zero(currency), |total, line| {
            total.checked_add(line.base_amount)
        })
        .unwrap_or_else(|_| Money::zero(currency))
}

/// `books · manual`, which is what rule 3 of the ledger buys.
fn source_of(journal: &Posted) -> String {
    format!("{} · {}", journal.source.app, journal.source.doc_type)
}
