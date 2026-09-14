//! One payment: the editor while it is a draft, the document once it is
//! posted.
//!
//! # The allocation half draws itself from what is owed
//!
//! Choosing a customer fetches their outstanding invoices, oldest due first,
//! and every one of them is a row with an empty box. Somebody types into two of
//! them, or presses *Settle in full* and edits down. The alternative - a picker
//! that adds one invoice at a time - makes the ordinary case, "this cheque
//! clears these four", four searches.
//!
//! A row left blank settles nothing. That is what makes a part payment one
//! number typed rather than a row removed.
//!
//! # What is left over is shown, never hidden
//!
//! Received less allocated is money on the customer's account, and it is a
//! legitimate state - a round sum against nothing in particular. It is on the
//! screen as its own figure so that it is a decision rather than a discrepancy
//! somebody notices on the statement three weeks later.
//!
//! # Posting is the irreversible half
//!
//! It takes a number, converts once at the rate for the day the money arrived,
//! writes the journal, and from then on the invoices it settles stop being
//! owed. A cheque that bounces is *withdrawn*, which reverses the entry and
//! keeps the number.

use app_books::payment::{
    AllocationInput, Payment, PaymentInput, PaymentStatus, PostOutcome, Settleable,
};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::permissions;
use phonix_master::party::{PartySummary, roles};
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::{
    blank_payment, cash_accounts, delete_payment, payment_detail, payment_journal, post_payment,
    save_payment, settleable_invoices, void_payment,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;
use crate::ui::table::DataGrid;
use crate::ui::table::config::payments::payments_grid;

const BACK: &str = "/sales/payments";

/// Recording one.
#[component]
pub fn payment_new_page() -> impl IntoView {
    // The blank comes from the server: its currency is the workspace's and its
    // account is whatever the `cash` role names, neither of which the browser
    // knows.
    let blank = Resource::new(|| (), |()| async move { blank_payment().await });

    view! {
        <Title text=format!("{} | Phonix", l!("payments.new")) />

        <PageHeader
            title=l!("payments.new")
            subtitle=l!("payments.new.subtitle")
            icon=Icon::Receipt
            back=(BACK, l!("payments.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match blank.await {
                    Ok(draft) => view! { <PaymentEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <Notice
                                message=Signal::derive(move || Some(err.to_string()))
                                tone=Tone::Danger
                            />
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
    }
}

/// One payment: editable while it is a draft, a document after.
#[component]
pub fn payment_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let payment_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let payment = Resource::new(payment_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => payment_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a payment id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.payment.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match payment.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let party = stored.party.name.clone();
                        let status = stored.status;
                        let opened_on = PaymentInput::from_payment(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=party
                                    icon=Icon::Receipt
                                    back=(BACK, l!("payments.title"))
                                >
                                    <StatusBadge status=status />
                                </PageHeader>

                                {if status.is_editable() {
                                    view! { <PaymentEditor draft=opened_on /> }.into_any()
                                } else {
                                    view! {
                                        <PaymentDocument
                                            payment=stored
                                            reload=Callback::new(move |()| payment.refetch())
                                        />
                                    }
                                        .into_any()
                                }}
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.payment.singular")
                                    icon=Icon::Receipt
                                    back=(BACK, l!("payments.title"))
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
fn status_badge(status: PaymentStatus) -> impl IntoView {
    let label = crate::i18n::t(&status.label());
    let tone = match status {
        PaymentStatus::Draft => Tone::Neutral,
        PaymentStatus::Posted => Tone::Success,
        PaymentStatus::Voided => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

/// What has been set against invoices, out of what was received.
///
/// Parsed from the boxes as somebody types, with the same arithmetic the server
/// uses - `phonix_core::money` compiles to wasm, which is the whole reason the
/// figure below the table is the figure that will be stored.
fn allocated_of(draft: &PaymentInput) -> Money {
    let amounts = draft.allocations.iter().filter_map(|line| {
        let typed = line.amount.trim();
        (!typed.is_empty())
            .then(|| Money::parse(draft.currency, typed).ok())
            .flatten()
    });

    Money::total(draft.currency, amounts).unwrap_or_else(|_| Money::zero(draft.currency))
}

#[component]
fn payment_editor(draft: PaymentInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let customers = Resource::new(
        || (),
        |()| async move { list_parties(Some(roles::CUSTOMER.to_owned())).await },
    );
    let accounts = Resource::new(|| (), |()| async move { cash_accounts().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let customers = customers.await.unwrap_or_default();
                    let accounts = accounts.await.unwrap_or_default();

                    let account_options = accounts
                        .into_iter()
                        .map(|account| {
                            Choice::new(account.id.to_string(), account.name)
                                .detail(account.number)
                        })
                        .collect::<Vec<_>>();

                    view! {
                        <EditorBody
                            draft=draft
                            customers=customers
                            accounts=account_options
                            saving=saving
                            rejected=rejected
                        />
                    }
                })}
            </Transition>
        </div>
    }
}

#[component]
fn editor_body(
    draft: RwSignal<PaymentInput>,
    customers: Vec<PartySummary>,
    accounts: Vec<Choice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let allocated = Memo::new(move |_| draft.with(allocated_of));
    let on_account = Memo::new(move |_| {
        draft.with(|d| {
            let amount = Money::parse(d.currency, d.amount.trim())
                .unwrap_or_else(|_| Money::zero(d.currency));

            amount
                .checked_sub(allocated_of(d))
                .unwrap_or_else(|_| Money::zero(d.currency))
        })
    });

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_payment(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("payments.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/sales/payments/{id}"),
                                leptos_router::NavigateOptions {
                                    replace: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                    Ok(Submission::Rejected(errors)) => {
                        rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                    }
                    Err(err) => rejected.set(Some(err.to_string())),
                }
            });
        }
    };

    let post = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("payments.post.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = post_payment(id).await;
                        saving.set(false);

                        match result {
                            Ok(PostOutcome::Posted { number }) => {
                                alerts.post(
                                    Alert::success(l!("payments.posted", number = number))
                                        .titled(l!("payments.post")),
                                );
                                navigate(
                                    &format!("/sales/payments/{id}"),
                                    leptos_router::NavigateOptions {
                                        replace: true,
                                        ..Default::default()
                                    },
                                );
                            }
                            Ok(other) => {
                                alerts.post(Alert::warning(crate::i18n::t(&other.message())));
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(l!("payments.post"))
                .confirm_label(l!("payments.post")),
            );
        }
    };

    let delete = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("payments.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_payment(id).await {
                            Ok(()) => {
                                alerts.post(Alert::success(l!("payments.deleted")));
                                navigate(BACK, leptos_router::NavigateOptions::default());
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(l!("common.delete"))
                .confirm_label(l!("common.delete")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());

    view! {
        <Panel>
            <Section title=l!("payments.header")>
                <HeaderFields draft=draft customers=customers accounts=accounts />
            </Section>

            <Section
                title=l!("payments.allocations")
                description=l!("payments.allocations.help")
            >
                <AllocationTable draft=draft />
            </Section>

            <div class="grid gap-3 lg:grid-cols-[1fr_22rem] lg:items-start">
                <Section title=l!("payments.note") flush=true>
                    <textarea
                        class="w-full"
                        rows="3"
                        prop:value=move || draft.with(|d| d.note.clone())
                        on:input=move |ev| {
                            let text = event_target_value(&ev);
                            draft.update(|d| d.note = text);
                        }
                    />
                </Section>

                <Section title=l!("payments.amount") flush=true>
                    <dl class="space-y-1 text-sm tabular-nums">
                        <Figure
                            label=l!("payments.allocated")
                            amount=Signal::derive(move || allocated.get())
                        />
                        // What is left over. Shown always rather than only when
                        // it is not nought: a figure that appears and
                        // disappears as somebody types is a figure they stop
                        // trusting.
                        <Figure
                            label=l!("payments.on_account")
                            amount=Signal::derive(move || on_account.get())
                            strong=true
                        />
                    </dl>
                </Section>
            </div>

            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("common.delete")
                        icon=Icon::Trash2
                        on_click=Callback::new({
                            let delete = delete.clone();
                            move |()| delete()
                        })
                    />
                </Show>

                <GhostButton
                    label=l!("common.save")
                    icon=Icon::Save
                    disabled=Signal::derive(move || saving.get())
                    on_click=Callback::new({
                        let save = save.clone();
                        move |()| save()
                    })
                />

                <Show when=saved fallback=|| ()>
                    <PrimaryButton
                        label=l!("payments.post")
                        icon=Icon::Check
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let post = post.clone();
                            move |()| post()
                        })
                    />
                </Show>
            </div>
        </Panel>
    }
}

/// One figure in the summary beside the note.
#[component]
fn figure(
    #[prop(into)] label: String,
    amount: Signal<Money>,
    #[prop(optional)] strong: bool,
) -> impl IntoView {
    let value = if strong {
        "font-medium text-content"
    } else {
        "text-content-muted"
    };

    view! {
        <div class="flex items-baseline justify-between gap-4">
            <dt class="text-content-muted">{label}</dt>
            <dd class=value>{move || amount.get().to_display_string()}</dd>
        </div>
    }
}

/// Who paid, into what, when, and how much.
#[component]
fn header_fields(
    draft: RwSignal<PaymentInput>,
    customers: Vec<PartySummary>,
    accounts: Vec<Choice>,
) -> impl IntoView {
    let customer_options = customers
        .iter()
        .filter(|party| party.is_active)
        .map(|party| {
            Choice::new(party.id.to_string(), party.name.clone()).detail(party.code.clone())
        })
        .collect::<Vec<_>>();
    let currency_options = Currency::all()
        .iter()
        .map(|currency| Choice::new(currency.code(), currency.label()))
        .collect::<Vec<_>>();

    view! {
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="block space-y-1">
                <label for="payment-customer" class="block text-xs font-medium text-content-muted">
                    {l!("payments.customer")}
                </label>
                <SelectField
                    id="payment-customer"
                    value=Signal::derive(move || {
                        draft.with(|d| d.party_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft
                            .update(|d| {
                                // A different customer is a different set of
                                // invoices, so what was set against the last
                                // one's is cleared rather than left pointing at
                                // somebody else's paperwork.
                                if d.party_id != chosen {
                                    d.allocations.clear();
                                }
                                d.party_id = chosen;
                            });
                    })
                    options=customer_options
                    placeholder=l!("common.not_set")
                    clearable=true
                />
            </div>

            <div class="block space-y-1">
                <label for="payment-account" class="block text-xs font-medium text-content-muted">
                    {l!("payments.account")}
                </label>
                <SelectField
                    id="payment-account"
                    value=Signal::derive(move || {
                        draft.with(|d| d.account_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft.update(|d| d.account_id = chosen);
                    })
                    options=accounts
                    placeholder=l!("common.not_set")
                    clearable=true
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("payments.account.help")}
                </span>
            </div>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("payments.received_on")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.received_on.to_string())
                    on:change=move |ev| {
                        if let Ok(date) = event_target_value(&ev).parse() {
                            draft.update(|d| d.received_on = date);
                        }
                    }
                />
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("payments.amount")}
                </span>
                // Text rather than a number input: `Money::parse` refuses a
                // third decimal place on a two-decimal currency rather than
                // rounding it, and a browser number input would have rounded
                // before this ever saw it.
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=move || draft.with(|d| d.amount.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.amount = value);
                    }
                />
            </label>

            <div class="block space-y-1">
                <label for="payment-currency" class="block text-xs font-medium text-content-muted">
                    {l!("field.currency")}
                </label>
                <SelectField
                    id="payment-currency"
                    value=Signal::derive(move || draft.with(|d| d.currency.code().to_owned()))
                    on_change=Callback::new(move |value: String| {
                        // A different currency is a different set of settleable
                        // invoices - only ones raised in it may be settled - so
                        // what was allocated is cleared with it.
                        if let Ok(currency) = Currency::parse(&value) {
                            draft
                                .update(|d| {
                                    if d.currency != currency {
                                        d.allocations.clear();
                                    }
                                    d.currency = currency;
                                });
                        }
                    })
                    options=currency_options
                />
            </div>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("payments.reference")}
                </span>
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.reference.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.reference = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("payments.reference.help")}
                </span>
            </label>
        </div>
    }
}

/// What this customer still owes, with a box against each.
#[component]
fn allocation_table(draft: RwSignal<PaymentInput>) -> impl IntoView {
    // Refetched when the customer or the currency changes, and when the payment
    // itself is saved: what is available depends on what other posted payments
    // have taken, and this screen's own allocations are excluded from that.
    let key = Signal::derive(move || {
        draft.with(|d| (d.party_id, d.currency.code().to_owned(), d.id))
    });

    let owed = Resource::new(
        move || key.get(),
        |(party_id, currency, editing)| async move {
            match party_id {
                Some(party_id) => settleable_invoices(party_id, currency, editing)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        },
    );

    view! {
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let owed: Vec<Settleable> = owed.await;

                if owed.is_empty() {
                    return view! {
                        <p class="py-4 text-sm text-content-subtle">
                            {l!("payments.allocations.none")}
                        </p>
                    }
                        .into_any();
                }

                view! {
                    <div class="overflow-x-auto">
                        <table class="w-full min-w-[40rem] text-sm">
                            <thead>
                                <tr class="border-b border-edge text-left text-xs text-content-muted">
                                    <th class="py-2 font-medium">{l!("field.number")}</th>
                                    <th class="py-2 font-medium">{l!("reports.column.date")}</th>
                                    <th class="py-2 font-medium">{l!("reports.column.due")}</th>
                                    <th class="py-2 text-right font-medium">
                                        {l!("payments.invoiced")}
                                    </th>
                                    <th class="py-2 text-right font-medium">
                                        {l!("payments.outstanding")}
                                    </th>
                                    <th class="w-32 py-2 text-right font-medium">
                                        {l!("payments.allocated")}
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                {owed
                                    .into_iter()
                                    .map(|invoice| {
                                        view! { <AllocationRow draft=draft invoice=invoice /> }
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                    .into_any()
            })}
        </Transition>
    }
}

#[component]
fn allocation_row(draft: RwSignal<PaymentInput>, invoice: Settleable) -> impl IntoView {
    let invoice_id = invoice.invoice_id;
    let outstanding = invoice.outstanding;

    let typed = move || {
        draft.with(|d| {
            d.allocations
                .iter()
                .find(|line| line.invoice_id == invoice_id)
                .map(|line| line.amount.clone())
                .unwrap_or_default()
        })
    };

    let set = move |amount: String| {
        draft.update(|d| {
            match d
                .allocations
                .iter_mut()
                .find(|line| line.invoice_id == invoice_id)
            {
                Some(line) => line.amount = amount,
                None => d.allocations.push(AllocationInput { invoice_id, amount }),
            }
        });
    };

    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1.5 font-mono text-xs text-content">{invoice.number.clone()}</td>
            <td class="py-1.5 text-content-muted">{invoice.issued_on.to_string()}</td>
            <td class="py-1.5 text-content-muted">
                {invoice.due_on.map(|due| due.to_string()).unwrap_or_default()}
            </td>
            <td class="py-1.5 pl-3 text-right tabular-nums text-content-muted">
                {invoice.invoiced.to_display_string()}
            </td>
            <td class="py-1.5 pl-3 text-right tabular-nums text-content">
                // Pressing the figure fills the box with it. The ordinary case
                // is settling an invoice in full, and asking somebody to retype
                // a number the screen is already showing them is asking them to
                // make a typing mistake.
                <button
                    type="button"
                    class="tabular-nums text-brand hover:underline"
                    title=l!("payments.settle_all")
                    on:click={
                        let full = outstanding.to_storage_string();
                        move |_| set(full.clone())
                    }
                >
                    {outstanding.to_display_string()}
                </button>
            </td>
            <td class="py-1.5 pl-3">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=typed
                    on:input=move |ev| set(event_target_value(&ev))
                />
            </td>
        </tr>
    }
}

// --- the document -------------------------------------------------------

/// A posted or withdrawn payment: read-only, and everything on it is what was
/// stored rather than what could be looked up now.
#[component]
fn payment_document(payment: Payment, reload: Callback<()>) -> impl IntoView {
    let alerts = Alerts::get();
    let viewer = crate::ui::viewer::Viewer::get();

    let may_void = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::PAYMENTS_VOID))
        })
    });

    let id = payment.id;
    let is_posted = payment.status == PaymentStatus::Posted;

    let party = payment.party.label();
    let received = payment.received_on.to_string();
    let account = format!("{} \u{b7} {}", payment.account_number, payment.account_name);
    let reference = payment.reference.clone();
    let note = payment.note.clone();
    let code = payment.currency.code().to_owned();
    let amount = payment.amount.to_display_string();
    let base = payment
        .base_amount
        .map(|base| format!("{} {}", base.currency().code(), base.to_display_string()));
    let on_account = payment.on_account().ok().filter(|left| !left.is_zero());
    let allocations = payment.allocations.clone();

    let void = move || {
        alerts.ask(
            Confirm::new(l!("payments.void.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match void_payment(id).await {
                        Ok(()) => {
                            alerts.post(Alert::success(l!("payments.voided")));
                            let _ = reload.try_run(());
                        }
                        Err(err) => alerts.post(Alert::failure(err.to_string())),
                    }
                });
            })
            .titled(l!("payments.void"))
            .confirm_label(l!("payments.void")),
        );
    };

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("payments.customer") flush=true>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{party}</div>
                        {reference
                            .map(|reference| {
                                view! {
                                    <div class="text-xs text-content-muted">
                                        {l!("payments.reference")} ": " {reference}
                                    </div>
                                }
                            })}
                    </div>
                </Section>

                <Section title=l!("payments.header") flush=true>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("payments.received_on")}</dt>
                            <dd class="tabular-nums text-content">{received}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("payments.account")}</dt>
                            <dd class="text-content">{account}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">
                                {l!("payments.amount")} " "
                                <span class="text-2xs text-content-subtle">{code}</span>
                            </dt>
                            <dd class="font-medium tabular-nums text-content">{amount}</dd>
                        </div>
                        {base
                            .map(|base| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("invoices.in_base")}
                                        </dt>
                                        <dd class="tabular-nums text-content-muted">{base}</dd>
                                    </div>
                                }
                            })}
                        {on_account
                            .map(|left| {
                                let text = left.to_display_string();

                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("payments.on_account")}
                                        </dt>
                                        <dd class="tabular-nums text-content">{text}</dd>
                                    </div>
                                }
                            })}
                        <JournalRow payment_id=id />
                    </dl>
                </Section>
            </div>

            <Section title=l!("payments.allocations")>
                {if allocations.is_empty() {
                    view! {
                        <p class="py-2 text-sm text-content-subtle">
                            {l!("payments.allocations.none")}
                        </p>
                    }
                        .into_any()
                } else {
                    view! {
                        <div class="overflow-x-auto">
                            <table class="w-full min-w-[32rem] text-sm">
                                <thead>
                                    <tr class="border-b border-edge text-left text-xs text-content-muted">
                                        <th class="py-2 font-medium">{l!("field.number")}</th>
                                        <th class="py-2 font-medium">
                                            {l!("reports.column.date")}
                                        </th>
                                        <th class="py-2 text-right font-medium">
                                            {l!("payments.invoiced")}
                                        </th>
                                        <th class="py-2 text-right font-medium">
                                            {l!("payments.allocated")}
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {allocations
                                        .into_iter()
                                        .map(|line| {
                                            let number = line
                                                .invoice_number
                                                .clone()
                                                .unwrap_or_default();
                                            let href = format!(
                                                "/sales/invoices/{}",
                                                line.invoice_id,
                                            );

                                            view! {
                                                <tr class="border-b border-edge/60">
                                                    <td class="py-1.5">
                                                        <a
                                                            class="font-mono text-xs text-brand hover:underline"
                                                            href=href
                                                        >
                                                            {number}
                                                        </a>
                                                    </td>
                                                    <td class="py-1.5 text-content-muted">
                                                        {line.issued_on.to_string()}
                                                    </td>
                                                    <td class="py-1.5 pl-3 text-right tabular-nums text-content-muted">
                                                        {line.invoiced.to_display_string()}
                                                    </td>
                                                    <td class="py-1.5 pl-3 text-right tabular-nums text-content">
                                                        {line.amount.to_display_string()}
                                                    </td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }
                        .into_any()
                }}
            </Section>

            {note
                .map(|note| {
                    view! {
                        <Section title=l!("payments.note")>
                            <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                        </Section>
                    }
                })}

            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=move || is_posted && may_void.get() fallback=|| ()>
                    <GhostButton
                        label=l!("payments.void")
                        icon=Icon::Ban
                        on_click=Callback::new(move |()| void())
                    />
                </Show>
            </div>

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::PAYMENT id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}

/// What this payment did to the ledger, and the way to go and read it.
///
/// Fetched rather than carried on the payment, for the reason the invoice's is:
/// the link is a fact about the journals table, and a payment that stored its
/// own journal id would be two records of one thing.
#[component]
fn journal_row(payment_id: Uuid) -> impl IntoView {
    let journal = Resource::new(
        move || payment_id,
        |payment_id| async move { payment_journal(payment_id).await.ok().flatten() },
    );

    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                journal
                    .await
                    .map(|(journal_id, number)| {
                        view! {
                            <div class="flex justify-between gap-4">
                                <dt class="text-content-muted">{l!("payments.journal")}</dt>
                                <dd>
                                    <a
                                        class="font-mono text-xs text-brand hover:underline"
                                        href=format!("/sales/journals/{journal_id}")
                                    >
                                        {number}
                                    </a>
                                </dd>
                            </div>
                        }
                    })
            })}
        </Transition>
    }
}

/// The list.
#[component]
pub fn payments_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("payments.title")) />

        <PageHeader
            title=l!("payments.title")
            subtitle=l!("payments.subtitle")
            icon=Icon::Receipt
        />

        <DataGrid config=payments_grid() />
    }
}
