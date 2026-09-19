//! Price lists: the list of them, and the one screen that edits one.
//!
//! # A list and its prices are one document
//!
//! Not a list screen and a separate price editor. A price list with nothing in
//! it is not a thing anybody wants, so the header and the rows are typed and
//! saved together - the shape a sales order already has, for the same reason.

use app_inventory::price_list::{ItemPriceInput, PriceListInput};
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::form::Submission;
use uuid::Uuid;

use crate::components::page::{GhostButton, Notice, PageHeader, Panel, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::pages::inventory::item_lookup::ItemLookup;
use crate::server_fns::inventory_fns::{blank_price_list, price_list_detail, save_price_list};
use crate::ui::alert::{Alert, Alerts};
use crate::ui::form::field::Choice;
use crate::ui::table::DataGrid;
use crate::ui::table::config::price_lists::price_lists_grid;

#[component]
pub fn price_lists_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("price_lists.title")) />

        <PageHeader
            title=l!("price_lists.title")
            subtitle=l!("price_lists.subtitle")
            icon=Icon::Receipt
        />

        <DataGrid config=price_lists_grid() />
    }
}

/// Adding one.
#[component]
pub fn price_list_new_page() -> impl IntoView {
    let blank = Resource::new(|| (), |()| async move { blank_price_list().await });

    view! {
        <Title text=format!("{} | Evrykit", l!("price_lists.new")) />

        <PageHeader
            title=l!("price_lists.new")
            subtitle=l!("price_lists.new.subtitle")
            icon=Icon::Receipt
            back=("/inventory/price-lists", l!("price_lists.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match blank.await {
                    Ok(draft) => view! { <PriceListEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        let message = err.to_string();
                        view! {
                            <Notice
                                message=Signal::derive(move || Some(message.clone()))
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

/// Editing one.
#[component]
pub fn price_list_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let id = move || params.with(|params| params.get("id").unwrap_or_default());

    let list = Resource::new(id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => price_list_detail(id).await.map_err(|err| err.to_string()),
            Err(_) => Err("That is not a price list id.".to_owned()),
        }
    });

    view! {
        <Title text=format!("{} | Evrykit", l!("price_lists.title")) />

        <PageHeader
            title=l!("price_lists.title")
            subtitle=l!("price_lists.subtitle")
            icon=Icon::Receipt
            back=("/inventory/price-lists", l!("price_lists.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match list.await {
                    Ok(draft) => view! { <PriceListEditor draft=draft /> }.into_any(),
                    Err(message) => {
                        view! {
                            <Notice
                                message=Signal::derive(move || Some(message.clone()))
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

/// The header and its rows.
#[component]
fn price_list_editor(draft: PriceListInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let save = move || {
        if saving.get_untracked() {
            return;
        }

        saving.set(true);
        let navigate = navigate.clone();

        leptos::task::spawn_local(async move {
            let result = save_price_list(draft.get_untracked()).await;
            saving.set(false);

            match result {
                Ok(Submission::Saved(_)) => {
                    alerts.post(Alert::success(l!("common.saved")));
                    navigate(
                        "/inventory/price-lists",
                        leptos_router::NavigateOptions::default(),
                    );
                }
                Ok(Submission::Rejected(errors)) => {
                    let message = errors
                        .first()
                        .map(|error| crate::i18n::t(&error.message))
                        .unwrap_or_else(|| l!("common.not_saved"));

                    alerts.post(Alert::warning(message));
                }
                Err(err) => alerts.post(Alert::warning(err.to_string())),
            }
        });
    };

    view! {
        <Panel>
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                <label class="block space-y-1">
                    <span class="text-xs font-medium text-content-muted">{l!("field.code")}</span>
                    <input
                        type="text"
                        class="w-full font-mono"
                        prop:value=move || draft.with(|d| d.code.clone())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            draft.update(|d| d.code = value);
                        }
                    />
                </label>

                <label class="block space-y-1 sm:col-span-2">
                    <span class="text-xs font-medium text-content-muted">{l!("field.name")}</span>
                    <input
                        type="text"
                        class="w-full"
                        prop:value=move || draft.with(|d| d.name.clone())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            draft.update(|d| d.name = value);
                        }
                    />
                </label>

                <label class="block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("field.currency")}
                    </span>
                    <input
                        type="text"
                        class="w-full uppercase"
                        maxlength="3"
                        prop:value=move || draft.with(|d| d.currency.clone())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            draft.update(|d| d.currency = value);
                        }
                    />
                </label>
            </div>

            <label class="mt-3 flex items-center gap-2 text-sm">
                <input
                    type="checkbox"
                    prop:checked=move || draft.with(|d| d.is_active)
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        draft.update(|d| d.is_active = on);
                    }
                />
                <span class="text-content">{l!("common.active")}</span>
            </label>

            <div class="mt-4 overflow-x-auto">
                <table class="w-full min-w-[46rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2"></th>
                            <th class="py-2 font-medium">{l!("field.item")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("price_lists.min_quantity")}
                            </th>
                            <th class="w-40 py-2 font-medium">{l!("price_lists.valid_from")}</th>
                            <th class="w-40 py-2 font-medium">{l!("price_lists.valid_to")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("price_lists.unit_price")}
                            </th>
                            <th class="w-8 py-2"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let count = draft.with(|d| d.prices.len());
                            (0..count)
                                .map(|index| view! { <PriceRow draft=draft index=index /> })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("price_lists.price.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.prices.push(ItemPriceInput::blank()));
                })
            />

            <div class="mt-4 flex justify-end border-t border-edge pt-4">
                <PrimaryButton
                    label=l!("common.save")
                    icon=Icon::Save
                    pending=Signal::derive(move || saving.get())
                    on_click=Callback::new(move |()| save())
                />
            </div>
        </Panel>
    }
}

/// One priced row.
#[component]
fn price_row(draft: RwSignal<PriceListInput>, index: usize) -> impl IntoView {
    let field = move |read: fn(&ItemPriceInput) -> String| {
        draft.with(|d| d.prices.get(index).map(read).unwrap_or_default())
    };

    let initial = draft.with_untracked(|d| {
        let row = d.prices.get(index)?;
        let id = row.variant_id?;

        Some(Choice::new(id.to_string(), id.to_string()))
    });

    let date = move |read: fn(&ItemPriceInput) -> Option<chrono::NaiveDate>| {
        draft.with(|d| {
            d.prices
                .get(index)
                .and_then(read)
                .map(|on| on.to_string())
                .unwrap_or_default()
        })
    };

    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1 text-xs text-content-subtle">{index + 1}</td>
            <td class="py-1 pr-2">
                <ItemLookup
                    initial=initial
                    sellable=true
                    on_pick=Callback::new(move |picked: Option<VariantChoice>| {
                        draft
                            .update(|d| {
                                if let Some(row) = d.prices.get_mut(index) {
                                    row.variant_id = picked.as_ref().map(|variant| variant.id);
                                }
                            });
                    })
                    placeholder=Some(l!("common.not_set"))
                />
            </td>
            <td class="py-1 pr-2">
                // Blank is any quantity, which is what a list without breaks is
                // made of - so the placeholder says so rather than a zero.
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    placeholder=l!("price_lists.any_quantity")
                    prop:value=move || field(|row| row.min_quantity.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(row) = d.prices.get_mut(index) {
                                    row.min_quantity = value;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || date(|row| row.valid_from)
                    on:change=move |ev| {
                        let on = event_target_value(&ev).parse().ok();
                        draft
                            .update(|d| {
                                if let Some(row) = d.prices.get_mut(index) {
                                    row.valid_from = on;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || date(|row| row.valid_to)
                    on:change=move |ev| {
                        let on = event_target_value(&ev).parse().ok();
                        draft
                            .update(|d| {
                                if let Some(row) = d.prices.get_mut(index) {
                                    row.valid_to = on;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=move || field(|row| row.unit_price.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(row) = d.prices.get_mut(index) {
                                    row.unit_price = value;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1">
                <GhostButton
                    label=l!("common.remove")
                    icon=Icon::Trash2
                    on_click=Callback::new(move |()| {
                        draft
                            .update(|d| {
                                if index < d.prices.len() {
                                    d.prices.remove(index);
                                }
                            });
                    })
                />
            </td>
        </tr>
    }
}
