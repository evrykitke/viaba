//! Account determination: which account each kind of posting lands on.
//!
//! # The screen the chart was missing
//!
//! Every sub-ledger names a *role* - stock, revenue, tax charged - and the
//! chart says which account that means here. Until now nothing could change
//! that mapping: it was installed with the chart and after that only SQL could
//! move it, so a role the defaults could not fill left its posting refused with
//! no way to fix it from inside the application.
//!
//! # Most of the chart is greyed out, on purpose
//!
//! The same rule the item's accounts panel applies, because it is the same
//! question. Two hundred accounts in a list, any of which posts, is how revenue
//! ends up in petty cash - and nothing downstream catches it, because a wrong
//! account balances exactly as well as a right one. So the ledger says which
//! accounts carry a role and the rest are shown unselectable: still findable,
//! so somebody hunting for the account they expected can see that it is there
//! and see that it does not fit.
//!
//! # Saving is per row
//!
//! One `<select>`, one write. There is no form to submit: a mapping is a single
//! decision, twelve of them are independent, and a screen that made somebody
//! press Save after changing one would be a screen that loses the other eleven
//! when the tab is closed.

use app_books::account::RoleMapping;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::i18n::Message;
use phonix_core::permissions;
use phonix_ports::ledger::{AccountRole, Fit, LedgerAccount};
use uuid::Uuid;

use crate::components::page::{PageHeader, Panel};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::{account_roles, role_chart, set_account_role};
use crate::ui::alert::{Alert, Alerts};

#[component]
pub fn account_roles_page() -> impl IntoView {
    let alerts = Alerts::get();
    let viewer = crate::ui::viewer::Viewer::get();

    let may_edit = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::ACCOUNTS_EDIT))
        })
    });

    // Bumped after every write. The row that changed could update itself, but
    // an account retired in another tab would then stay on screen under a role
    // that no longer resolves - and this screen exists to tell the truth about
    // exactly that.
    let reload = RwSignal::new(0_u32);

    let mappings = Resource::new(
        move || reload.get(),
        |_| async move { account_roles().await.unwrap_or_default() },
    );

    // Fetched once. The chart does not change while somebody is mapping roles
    // against it, and refetching two hundred accounts per row would be two
    // hundred accounts per row.
    let chart = Resource::new(
        || (),
        |()| async move { role_chart().await.unwrap_or_default() },
    );

    let choose = Callback::new(move |(role, account_id): (AccountRole, Option<Uuid>)| {
        leptos::task::spawn_local(async move {
            match set_account_role(role, account_id).await {
                Ok(()) => {
                    alerts.post(Alert::success(l!("account_roles.saved")));
                    reload.update(|count| *count = count.wrapping_add(1));
                }
                Err(err) => {
                    alerts.post(Alert::failure(err.to_string()));
                    // Put the select back to what is actually stored. A control
                    // left showing a refused choice is a control that lies.
                    reload.update(|count| *count = count.wrapping_add(1));
                }
            }
        });
    });

    view! {
        <Title text=format!("{} | Evrykit", l!("account_roles.title")) />

        <PageHeader
            title=l!("account_roles.title")
            subtitle=l!("account_roles.subtitle")
            icon=Icon::ListTree
            back=("/accounting/accounts", l!("accounts.title"))
        />

        <Panel>
            <div class="space-y-3">
                <p class="text-xs text-content-subtle">{l!("account_roles.intro")}</p>

                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        let chart = StoredValue::new(chart.await);
                        let rows = mappings.await;

                        rows.into_iter()
                            .map(|mapping| {
                                view! {
                                    <RoleRow
                                        mapping=mapping
                                        chart=chart
                                        may_edit=may_edit
                                        choose=choose
                                    />
                                }
                            })
                            .collect_view()
                    })}
                </Transition>
            </div>
        </Panel>
    }
}

/// One role: what it needs, what it posts to now, and the way to change it.
#[component]
fn role_row(
    mapping: RoleMapping,
    chart: StoredValue<Vec<LedgerAccount>>,
    may_edit: Signal<bool>,
    choose: Callback<(AccountRole, Option<Uuid>)>,
) -> impl IntoView {
    let role = mapping.role;
    let label = crate::i18n::t(&Message::new(format!("ledger.role.{}", role.as_str())));
    let wants = crate::i18n::t(&Message::new(format!(
        "ledger.role.{}.wants",
        role.as_str()
    )));

    let unmapped = !mapping.is_mapped();
    let selected = mapping
        .account_id
        .map(|id| id.to_string())
        .unwrap_or_default();

    // The three groups, in the order somebody reads them: what fits, what is
    // defensible, and what the ledger will not take.
    let group = move |title: String, fit: Option<Fit>| {
        let accounts: Vec<LedgerAccount> = chart
            .get_value()
            .into_iter()
            .filter(|account| account.fit_for(role) == fit)
            .collect();

        (!accounts.is_empty()).then(|| {
            let unsuited = fit.is_none();

            view! {
                <optgroup label=title>
                    {accounts
                        .into_iter()
                        .map(|account| {
                            let id = account.id.to_string();
                            let text = format!("{} \u{b7} {}", account.number, account.name);

                            view! { <option value=id disabled=unsuited>{text}</option> }
                        })
                        .collect_view()}
                </optgroup>
            }
        })
    };

    view! {
        <div class="grid gap-1.5 border-b border-edge pb-3 last:border-0 last:pb-0 sm:grid-cols-[1fr_2fr] sm:items-start sm:gap-3">
            <div class="min-w-0">
                <p class="text-sm text-content">{label}</p>
                <p class="text-xs text-content-subtle">{wants}</p>
                <Show when=move || unmapped fallback=|| ()>
                    <p class="text-xs text-danger">{l!("account_roles.unmapped")}</p>
                </Show>
            </div>

            <select
                class="h-8 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content disabled:text-content-subtle"
                disabled=move || !may_edit.get()
                prop:value=selected
                on:change=move |ev| {
                    let raw = event_target_value(&ev);
                    choose.run((role, raw.parse::<Uuid>().ok()));
                }
            >
                <option value="">{l!("account_roles.none")}</option>
                {group(l!("items.accounts.suited"), Some(Fit::Best))}
                {group(l!("items.accounts.possible"), Some(Fit::Allowed))}
                {group(l!("items.accounts.not_suited"), None)}
            </select>
        </div>
    }
}
