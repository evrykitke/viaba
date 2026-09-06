//! Posting a journal by hand.
//!
//! # Two money columns, and a difference that has to reach zero
//!
//! The Post button stays disabled until the debits and the credits agree. That
//! is not a courtesy check duplicating the server's - the server's is a type
//! that cannot be constructed unbalanced - it is that somebody typing a journal
//! wants to watch the difference shrink as they work, and finding out at submit
//! is finding out too late.
//!
//! One amount box per side, and typing in one clears the other, because a line
//! is a debit or a credit and never both. That is the model's side-plus-
//! magnitude on the screen rather than a signed number somebody has to reason
//! about.
//!
//! # The currency opens on the workspace's own
//!
//! The accountant set the base currency and it is preselected. Choosing another
//! is allowed - the field is never locked - and asks first, because a
//! foreign-currency journal needs a rate on file for its date and that is worth
//! knowing before twelve lines have been typed.

use app_books::account::{Account, Side};
use app_books::journal::{JournalDraft, JournalDraftLine};
use chrono::{NaiveDate, Utc};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use uuid::Uuid;

use crate::components::page::{PageHeader, Tone};
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::server_fns::books_fns::{JournalContext, journal_context, post_journal};
use crate::ui::alert::{Alert, Alerts, Confirm};

/// How many blank rows a new journal opens with. Two, because that is the
/// smallest journal there is.
const OPENING_ROWS: usize = 2;

#[component]
pub fn journal_new_page() -> impl IntoView {
    let context = Resource::new(|| (), |()| async move { journal_context().await });

    view! {
        <Title text=format!("{} | Phonix", l!("journals.new")) />

        <PageHeader
            title=l!("journals.new")
            subtitle=l!("journals.new.subtitle")
            icon=Icon::ScrollText
            back=("/sales/journals", l!("journals.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match context.await {
                    Ok(context) => view! { <JournalEditor context=context /> }.into_any(),
                    Err(err) => {
                        view! { <p class="text-sm text-danger">{err.to_string()}</p> }.into_any()
                    }
                }
            })}
        </Transition>
    }
}

#[component]
fn journal_editor(context: JournalContext) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let base = StoredValue::new(context.base_currency.clone());
    let accounts = StoredValue::new(context.accounts.clone());
    let centres = StoredValue::new(context.cost_centres.clone());
    let currencies = StoredValue::new(context.currencies.clone());

    let draft = RwSignal::new(JournalDraft {
        entry_date: Some(Utc::now().date_naive()),
        narration: String::new(),
        currency: Some(context.base_currency.clone()),
        lines: vec![JournalDraftLine::default(); OPENING_ROWS],
    });

    let sending = RwSignal::new(false);

    // Recomputed on every keystroke, which is the point: the difference is what
    // somebody is watching while they type.
    let totals = Memo::new(move |_| {
        let currency = draft.with(|draft| {
            draft
                .currency
                .as_deref()
                .and_then(|code| Currency::parse(code).ok())
                .unwrap_or_default()
        });

        draft.with(|draft| {
            let sum = |side: Side| {
                draft
                    .lines
                    .iter()
                    .filter(|line| line.side == Some(side))
                    .filter_map(|line| Money::parse(currency, line.amount.trim()).ok())
                    .try_fold(Money::zero(currency), |total, amount| {
                        total.checked_add(amount)
                    })
                    .unwrap_or_else(|_| Money::zero(currency))
            };

            (sum(Side::Debit), sum(Side::Credit))
        })
    });

    let balanced = Memo::new(move |_| {
        let (debits, credits) = totals.get();
        debits == credits && !debits.is_zero()
    });

    let ready = Memo::new(move |_| {
        balanced.get()
            && draft.with(|draft| {
                !draft.narration.trim().is_empty()
                    && draft.entry_date.is_some()
                    && draft.filled_lines().count() >= 2
            })
    });

    let post = move |_| {
        if sending.get_untracked() {
            return;
        }

        sending.set(true);
        let submitted = draft.get_untracked();
        let navigate = navigate.clone();

        leptos::task::spawn_local(async move {
            match post_journal(submitted).await {
                Ok(posted) => {
                    alerts.post(Alert::success(l!("journals.posted", number = posted.number)));
                    navigate(&format!("/sales/journals/{}", posted.id), Default::default());
                }
                Err(err) => {
                    alerts.post(Alert::failure(err.to_string()));
                    sending.set(false);
                }
            }
        });
    };

    view! {
        <div class="space-y-4">
            <JournalHeader draft=draft base=base currencies=currencies />
            <LineTable draft=draft accounts=accounts centres=centres />
            <Totals totals=totals balanced=balanced />

            <div class="flex items-center justify-end gap-2">
                <button
                    type="button"
                    class="flex h-9 items-center gap-1.5 rounded-control bg-brand px-3 text-sm font-medium text-on-brand disabled:opacity-50"
                    disabled=move || !ready.get() || sending.get()
                    on:click=post
                >
                    <Icon icon=Icon::Check size=IconSize::Sm />
                    {move || {
                        if sending.get() { l!("common.saving") } else { l!("journals.post") }
                    }}
                </button>
            </div>
        </div>
    }
}

/// Date, narration, and the currency the amounts are typed in.
#[component]
fn journal_header(
    draft: RwSignal<JournalDraft>,
    base: StoredValue<String>,
    currencies: StoredValue<Vec<Currency>>,
) -> impl IntoView {
    let alerts = Alerts::get();

    // Changing away from the workspace's own currency asks first. The change is
    // applied only if the person confirms, so the select falling back to what
    // the draft still holds is the right behaviour on a decline.
    let choose_currency = move |event: leptos::ev::Event| {
        let chosen = event_target_value(&event);
        let own = base.get_value();

        if chosen == own {
            draft.update(|draft| draft.currency = Some(chosen));
            return;
        }

        let confirmed = chosen.clone();
        let question = l!(
            "journals.currency.confirm",
            currency = chosen.clone(),
            base = own.clone()
        );

        alerts.ask(
            Confirm::new(question, move || {
                draft.update(|draft| draft.currency = Some(confirmed.clone()));
            })
            .titled(l!("journals.currency.title"))
            .confirm_label(l!("journals.currency.go_ahead"))
            .tone(Tone::Warning),
        );
    };

    view! {
        <div class="grid gap-3 rounded-card border border-edge bg-surface-raised p-4 sm:grid-cols-4">
            <label class="space-y-1">
                <span class="block text-xs font-medium text-content-muted">
                    {l!("journals.entry_date")}
                </span>
                <input
                    type="date"
                    class="h-9 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    prop:value=move || {
                        draft.with(|draft| draft.entry_date.map(|date| date.to_string()))
                    }
                    on:input=move |event| {
                        let parsed = NaiveDate::parse_from_str(
                                &event_target_value(&event),
                                "%Y-%m-%d",
                            )
                            .ok();
                        draft.update(|draft| draft.entry_date = parsed);
                    }
                />
            </label>

            <label class="space-y-1 sm:col-span-2">
                <span class="block text-xs font-medium text-content-muted">
                    {l!("journals.narration")}
                </span>
                <input
                    type="text"
                    class="h-9 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    placeholder=l!("journals.narration.placeholder")
                    prop:value=move || draft.with(|draft| draft.narration.clone())
                    on:input=move |event| {
                        let text = event_target_value(&event);
                        draft.update(|draft| draft.narration = text);
                    }
                />
            </label>

            <label class="space-y-1">
                <span class="block text-xs font-medium text-content-muted">
                    {l!("field.currency")}
                </span>
                <select
                    class="h-9 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    prop:value=move || {
                        draft.with(|draft| draft.currency.clone().unwrap_or_default())
                    }
                    on:change=choose_currency
                >
                    {move || {
                        let own = base.get_value();
                        currencies
                            .get_value()
                            .into_iter()
                            .map(|currency| {
                                let code = currency.code();
                                let label = if code == own {
                                    format!("{code} · {}", l!("journals.currency.own"))
                                } else {
                                    code.to_owned()
                                };
                                view! { <option value=code>{label}</option> }
                            })
                            .collect::<Vec<_>>()
                    }}
                </select>
            </label>
        </div>
    }
}

/// The lines, with a debit column and a credit column.
#[component]
fn line_table(
    draft: RwSignal<JournalDraft>,
    accounts: StoredValue<Vec<Account>>,
    centres: StoredValue<Vec<phonix_ports::CostCentre>>,
) -> impl IntoView {
    let add_row = move |_| {
        draft.update(|draft| draft.lines.push(JournalDraftLine::default()));
    };

    // Hoisted out of the view: a turbofish inside an attribute value is parsed
    // as a tag by the view macro, so the generics stay out here.
    let rows = move || -> Vec<usize> {
        let count = draft.with(|draft| draft.lines.len());
        (0..count).collect()
    };

    view! {
        <div class="overflow-x-auto rounded-card border border-edge bg-surface-raised">
            <table class="w-full min-w-[56rem] text-sm">
                <thead class="border-b border-edge text-xs text-content-subtle">
                    <tr>
                        <th class="px-3 py-2 text-left font-medium">{l!("field.account")}</th>
                        <th class="px-3 py-2 text-left font-medium">
                            {l!("journals.dimension.cost_centre")}
                        </th>
                        <th class="px-3 py-2 text-left font-medium">{l!("journals.memo")}</th>
                        <th class="px-3 py-2 text-right font-medium">{l!("books.side.debit")}</th>
                        <th class="px-3 py-2 text-right font-medium">{l!("books.side.credit")}</th>
                        <th class="w-8"></th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=rows
                        key=|index| *index
                        let:index
                    >
                        <LineRow draft=draft index=index accounts=accounts centres=centres />
                    </For>
                </tbody>
            </table>

            <div class="border-t border-edge p-2">
                <button
                    type="button"
                    class="flex h-8 items-center gap-1.5 rounded-control px-2 text-xs text-content-muted hover:bg-surface-hover hover:text-content"
                    on:click=add_row
                >
                    <Icon icon=Icon::Plus size=IconSize::Xs />
                    {l!("journals.add_line")}
                </button>
            </div>
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<JournalDraft>,
    index: usize,
    accounts: StoredValue<Vec<Account>>,
    centres: StoredValue<Vec<phonix_ports::CostCentre>>,
) -> impl IntoView {
    let account_value = move || {
        draft.with(|draft| {
            draft
                .lines
                .get(index)
                .and_then(|line| line.account_id)
                .map(|id| id.to_string())
                .unwrap_or_default()
        })
    };

    let centre_value = move || {
        draft.with(|draft| {
            draft
                .lines
                .get(index)
                .and_then(|line| line.cost_centre_id)
                .map(|id| id.to_string())
                .unwrap_or_default()
        })
    };

    let memo_value =
        move || draft.with(|draft| draft.lines.get(index).map(|line| line.memo.clone())
            .unwrap_or_default());

    // Typing in one side clears the other: a line is a debit or a credit, never
    // both, and this is the model's side-plus-magnitude on screen.
    let amount_for = move |side: Side| {
        draft.with(|draft| {
            draft
                .lines
                .get(index)
                .filter(|line| line.side == Some(side))
                .map(|line| line.amount.clone())
                .unwrap_or_default()
        })
    };

    let set_account = move |event: leptos::ev::Event| {
        let chosen = Uuid::parse_str(&event_target_value(&event)).ok();
        draft.update(|draft| {
            if let Some(line) = draft.lines.get_mut(index) {
                line.account_id = chosen;
            }
        });
    };

    let set_centre = move |event: leptos::ev::Event| {
        let chosen = Uuid::parse_str(&event_target_value(&event)).ok();
        draft.update(|draft| {
            if let Some(line) = draft.lines.get_mut(index) {
                line.cost_centre_id = chosen;
            }
        });
    };

    let set_memo = move |event: leptos::ev::Event| {
        let text = event_target_value(&event);
        draft.update(|draft| {
            if let Some(line) = draft.lines.get_mut(index) {
                line.memo = text.clone();
            }
        });
    };

    let set_debit = move |event: leptos::ev::Event| {
        let text = event_target_value(&event);
        draft.update(|draft| {
            if let Some(line) = draft.lines.get_mut(index) {
                line.side = (!text.trim().is_empty()).then_some(Side::Debit);
                line.amount = text.clone();
            }
        });
    };

    let set_credit = move |event: leptos::ev::Event| {
        let text = event_target_value(&event);
        draft.update(|draft| {
            if let Some(line) = draft.lines.get_mut(index) {
                line.side = (!text.trim().is_empty()).then_some(Side::Credit);
                line.amount = text.clone();
            }
        });
    };

    let clear = move |_| {
        // Never below the opening rows: a journal of one line cannot balance,
        // and an empty table is a screen somebody has to work out how to
        // restart.
        draft.update(|draft| {
            if draft.lines.len() > OPENING_ROWS {
                draft.lines.remove(index);
            } else if let Some(line) = draft.lines.get_mut(index) {
                *line = JournalDraftLine::default();
            }
        });
    };

    view! {
        <tr class="border-b border-edge last:border-b-0">
            <td class="px-3 py-1.5">
                <select
                    class="h-8 w-full min-w-[12rem] rounded-control border border-edge bg-surface px-2 text-sm"
                    prop:value=account_value
                    on:change=set_account
                >
                    <option value="">{l!("journals.choose_account")}</option>
                    {move || {
                        accounts
                            .get_value()
                            .into_iter()
                            .map(|account| {
                                view! {
                                    <option value=account.id
                                        .to_string()>{account.label()}</option>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </select>
            </td>

            <td class="px-3 py-1.5">
                <select
                    class="h-8 w-full min-w-[10rem] rounded-control border border-edge bg-surface px-2 text-sm"
                    prop:value=centre_value
                    on:change=set_centre
                >
                    <option value="">{l!("common.none")}</option>
                    {move || {
                        centres
                            .get_value()
                            .into_iter()
                            .map(|centre| {
                                view! {
                                    <option value=centre.id.to_string()>{centre.label()}</option>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </select>
            </td>

            <td class="px-3 py-1.5">
                <input
                    type="text"
                    class="h-8 w-full min-w-[10rem] rounded-control border border-edge bg-surface px-2 text-sm"
                    prop:value=memo_value
                    on:input=set_memo
                />
            </td>

            <td class="px-3 py-1.5">
                <input
                    type="text"
                    inputmode="decimal"
                    class="h-8 w-28 rounded-control border border-edge bg-surface px-2 text-right text-sm tabular-nums"
                    prop:value=move || amount_for(Side::Debit)
                    on:input=set_debit
                />
            </td>

            <td class="px-3 py-1.5">
                <input
                    type="text"
                    inputmode="decimal"
                    class="h-8 w-28 rounded-control border border-edge bg-surface px-2 text-right text-sm tabular-nums"
                    prop:value=move || amount_for(Side::Credit)
                    on:input=set_credit
                />
            </td>

            <td class="px-1">
                <button
                    type="button"
                    class="grid size-6 place-items-center rounded-control text-content-subtle hover:bg-surface-hover hover:text-danger"
                    aria-label=l!("journals.remove_line")
                    on:click=clear
                >
                    <Icon icon=Icon::X size=IconSize::Xs />
                </button>
            </td>
        </tr>
    }
}

/// Both sides, and the gap between them.
#[component]
fn totals(totals: Memo<(Money, Money)>, balanced: Memo<bool>) -> impl IntoView {
    view! {
        <div class="flex flex-wrap items-center justify-end gap-4 rounded-card border border-edge bg-surface-sunken px-4 py-2.5 text-sm">
            <span class="mr-auto text-xs text-content-subtle">{l!("journals.totals")}</span>

            <span class="tabular-nums text-content">
                {move || totals.get().0.to_display_string()}
            </span>
            <span class="tabular-nums text-content">
                {move || totals.get().1.to_display_string()}
            </span>

            <span class=move || {
                if balanced.get() {
                    "flex items-center gap-1.5 text-xs font-medium text-success"
                } else {
                    "flex items-center gap-1.5 text-xs font-medium text-warning"
                }
            }>
                {move || {
                    if balanced.get() {
                        view! {
                            <Icon icon=Icon::Check size=IconSize::Xs />
                            <span>{l!("journals.balanced")}</span>
                        }
                            .into_any()
                    } else {
                        let (debits, credits) = totals.get();
                        let difference = debits
                            .checked_sub(credits)
                            .unwrap_or_else(|_| Money::zero(debits.currency()));

                        view! {
                            <Icon icon=Icon::CircleAlert size=IconSize::Xs />
                            <span>
                                {l!(
                                    "journals.out_by", amount = difference.abs()
                                    .to_display_string()
                                )}
                            </span>
                        }
                            .into_any()
                    }
                }}
            </span>
        </div>
    }
}
