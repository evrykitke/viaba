//! One item: what it is, what it is offered in, what it looks like, and where
//! its money lands.
//!
//! # Why this has tabs
//!
//! An item is four things edited by four different people at four different
//! moments. Purchasing corrects a lead time. Merchandising adds a colour.
//! Somebody with a camera adds the photograph. Accounts points its revenue at a
//! different account for one financial year. One save button over all of that
//! would mean a stale draft reverting somebody else's correction as a side
//! effect, so each tab writes its own rows.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::i18n::Message;
use phonix_core::permissions;
use phonix_ports::ledger::{AccountRole, LedgerAccount};
use uuid::Uuid;

use app_inventory::accounts::{AccountOverrides, AccountRef};
use app_inventory::item::ItemInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Panel, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    item_accounts, item_edit, postable_accounts, selectable_categories, selectable_units,
    set_item_account,
};
use crate::ui::alert::{Alert, Alerts};
use crate::ui::form::EntityForm;
use crate::ui::form::config::items::item_form;
use crate::ui::tabs::{Tab, TabbedPanel};

use super::pictures::PicturesPanel;
use super::variants::VariantsPanel;

#[component]
pub fn item_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let item_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(item_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => item_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not an item id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.item.singular")) />

        // Transition, not Suspense: moving between items re-suspends, and a
        // fallback would blank the screen somebody is looking at.
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <ItemEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.item.singular")
                                    icon=Icon::Package
                                    back=("/inventory/items", l!("items.title"))
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
fn item_editor(draft: ItemInput) -> impl IntoView {
    let item_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = draft.name.clone();
    let code = draft.code.clone();
    let kind = crate::i18n::t(&draft.kind.label());
    let is_tracked = draft.is_tracked;
    let is_active = draft.is_active;

    // Hoisted above the tab strip: a tab's render closure runs again each time
    // it comes back on screen, so a resource declared inside one would be
    // refetched by every visit to it.
    let categories = Resource::new(|| (), |()| async move { selectable_categories().await });
    let units = Resource::new(|| (), |()| async move { selectable_units().await });
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", "Details", move || {
        view! {
            // No measure of its own: the form asks for the whole width, and a
            // wrapper here would take back what it asked for.
            <Panel>
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        let categories = categories.await.unwrap_or_default();
                        let units = units.await.unwrap_or_default();

                        view! {
                            <EntityForm
                                config=item_form(categories, units)
                                value=value.get_untracked()
                            />
                        }
                    })}
                </Transition>
            </Panel>
        }
        .into_any()
    })
    .icon(Icon::SlidersHorizontal);

    let variants_tab = Tab::new("variants", "Variants", move || {
        view! { <VariantsPanel item_id=item_id /> }.into_any()
    })
    .icon(Icon::Blocks)
    .require(permissions::ITEMS_EDIT);

    let pictures_tab = Tab::new("pictures", "Pictures", move || {
        view! { <PicturesPanel item_id=item_id /> }.into_any()
    })
    .icon(Icon::Image);

    let accounting_tab = Tab::new("accounting", "Accounting", move || {
        view! { <AccountsPanel item_id=item_id /> }.into_any()
    })
    .icon(Icon::Receipt)
    .require(permissions::ITEMS_EDIT);

    let history_tab = Tab::new("history", "History", move || {
        view! { <RecordHistory kind=kinds::ITEM id=Some(item_id.to_string()) /> }.into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader title=title icon=Icon::Package back=("/inventory/items", l!("items.title"))>
            <div class="flex flex-wrap items-center gap-1.5">
                <Badge label=code />
                <Badge label=kind tone=Tone::Brand />
                {(!is_tracked).then(|| view! { <Badge label=l!("items.not_counted") /> })}
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel
            id="item"
            tabs=vec![
                details_tab,
                variants_tab,
                pictures_tab,
                accounting_tab,
                history_tab,
            ]
        />
    }
}

/// Where this item's postings land.
///
/// # Almost every row here says "the default"
///
/// And that is the design working, not an empty screen. A posting falls through
/// the item to its category to the role Books seeded, and a workspace that
/// never opens this tab posts to a chart that is already right. What this tab
/// is for is the single item that needs its own revenue account - the one case
/// where forcing a whole category into existence to hold it would be worse.
#[component]
fn accounts_panel(item_id: Uuid) -> impl IntoView {
    let alerts = Alerts::get();
    let mapping = RwSignal::new((AccountOverrides::default(), AccountOverrides::default()));
    let chart = Resource::new(|| (), |()| async move { postable_accounts().await });

    let reload = Callback::new(move |()| {
        leptos::task::spawn_local(async move {
            if let Ok(fresh) = item_accounts(item_id).await {
                mapping.set(fresh);
            }
        });
    });

    Effect::new(move |_| reload.run(()));

    let choose = Callback::new(move |(role, chosen): (AccountRole, Option<AccountRef>)| {
        leptos::task::spawn_local(async move {
            let saved =
                set_item_account("item".to_owned(), item_id, role.as_str().to_owned(), chosen)
                    .await;

            match saved {
                Ok(()) => reload.run(()),
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
        });
    });

    view! {
        <div class="max-w-3xl">
            <Panel title=l!("items.accounts") description=l!("items.accounts_help")>
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        // No ledger, or none this caller may read: the pickers
                        // are empty and every role reads "the default", which
                        // is what it would have done anyway.
                        let chart = chart.await.unwrap_or_default();
                        let chart = StoredValue::new(chart);

                        view! {
                            <div class="space-y-3">
                                {AccountOverrides::OVERRIDABLE
                                    .iter()
                                    .copied()
                                    .map(|role| {
                                        view! {
                                            <AccountRow
                                                role=role
                                                mapping=mapping
                                                chart=chart
                                                choose=choose
                                            />
                                        }
                                    })
                                    .collect_view()}

                                <p class="text-xs text-content-subtle">
                                    {l!("items.accounts.category_note")}
                                </p>
                            </div>
                        }
                    })}
                </Transition>
            </Panel>
        </div>
    }
}

/// One role: what it posts to now, and where that answer came from.
#[component]
fn account_row(
    role: AccountRole,
    mapping: RwSignal<(AccountOverrides, AccountOverrides)>,
    chart: StoredValue<Vec<LedgerAccount>>,
    choose: Callback<(AccountRole, Option<AccountRef>)>,
) -> impl IntoView {
    let label = crate::i18n::t(&Message::new(format!("ledger.role.{}", role.as_str())));

    // Where the answer comes from, in the order a posting resolves it.
    let source = move || {
        let (item, category) = mapping.get();

        if item.for_role(role).is_some() {
            crate::i18n::t(&Message::new("items.accounts.from_item"))
        } else if category.for_role(role).is_some() {
            crate::i18n::t(&Message::new("items.accounts.from_category"))
        } else {
            crate::i18n::t(&Message::new("items.accounts.from_role"))
        }
    };

    let selected = move || {
        mapping
            .get()
            .0
            .for_role(role)
            .map(|chosen| chosen.account_id.to_string())
            .unwrap_or_default()
    };

    view! {
        <div class="grid gap-1.5 border-b border-edge pb-3 last:border-0 last:pb-0 sm:grid-cols-[1fr_2fr] sm:items-center sm:gap-3">
            <div class="min-w-0">
                <p class="text-sm text-content">{label}</p>
                <p class="text-xs text-content-subtle">{source}</p>
            </div>

            <select
                class="h-8 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                prop:value=selected
                on:change=move |ev| {
                    let raw = event_target_value(&ev);
                    let chosen = chart
                        .get_value()
                        .into_iter()
                        .find(|account| account.id.to_string() == raw)
                        .map(|account| AccountRef {
                            account_id: account.id,
                            number: account.number,
                            name: account.name,
                        });

                    choose.run((role, chosen));
                }
            >
                <option value="">{l!("items.accounts.default")}</option>
                {chart
                    .get_value()
                    .into_iter()
                    .map(|account| {
                        let id = account.id.to_string();
                        let text = format!("{} \u{b7} {}", account.number, account.name);

                        view! { <option value=id>{text}</option> }
                    })
                    .collect_view()}
            </select>
        </div>
    }
}
