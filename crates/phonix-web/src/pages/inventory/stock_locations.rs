//! Stock locations: the list, adding one, and editing one.
//!
//! The list is a tree and reads as one, because a location's identity is where
//! it sits: `WH/Stock/Zone A` is a different shelf from `WH2/Stock/Zone A` and
//! the leaf names are the same.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_inventory::location::LocationInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    list_stock_locations, selectable_warehouses, stock_location_edit,
};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::stock_locations::stock_location_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::stock_locations::stock_locations_grid;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn stock_locations_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("locations.title")) />

        <PageHeader
            title=l!("locations.title")
            subtitle=l!("locations.subtitle")
            icon=Icon::Boxes
        />

        <DataGrid config=stock_locations_grid() />
    }
}

/// Adding a location.
#[component]
pub fn stock_location_new_page() -> impl IntoView {
    let locations = Resource::new(|| (), |()| async move { list_stock_locations().await });
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });

    view! {
        <Title text=format!("{} | Phonix", l!("locations.new")) />

        <PageHeader
            title=l!("locations.new")
            subtitle=l!("locations.new.subtitle")
            icon=Icon::Boxes
            back=("/inventory/locations", l!("locations.title"))
        />

        <div class="max-w-3xl">
            <CollapsibleCard
                title=l!("locations.new")
                detail=l!("locations.new.subtitle")
                icon=Icon::Boxes
                open=true
            >
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        // A failed picker is an empty picker: a location with no
                        // parent is top level, and one with no warehouse is a
                        // counterpart rather than a shelf.
                        let locations = locations.await.unwrap_or_default();
                        let warehouses = warehouses.await.unwrap_or_default();

                        view! {
                            <EntityForm
                                config=stock_location_form(None, locations, warehouses)
                                value=LocationInput::blank()
                            />
                        }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}

/// One location: where it sits, and what has been done to it.
#[component]
pub fn stock_location_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let location_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(location_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => stock_location_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not a location id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.stock_location.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <StockLocationEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.stock_location.singular")
                                    icon=Icon::Boxes
                                    back=("/inventory/locations", l!("locations.title"))
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
fn stock_location_editor(draft: LocationInput) -> impl IntoView {
    let location_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = draft.name.clone();
    let kind = crate::i18n::t(&draft.kind.label());
    let is_active = draft.is_active;

    // Hoisted above the tab strip: a tab's render closure runs again each time
    // it comes back on screen.
    let locations = Resource::new(|| (), |()| async move { list_stock_locations().await });
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", "Details", move || {
        view! {
            <div class="max-w-3xl">
                <CollapsibleCard title=l!("locations.edit") icon=Icon::Boxes open=true>
                    <Transition fallback=|| {
                        view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                    }>
                        {move || Suspend::new(async move {
                            let locations = locations.await.unwrap_or_default();
                            let warehouses = warehouses.await.unwrap_or_default();

                            view! {
                                <EntityForm
                                    config=stock_location_form(
                                        Some(location_id),
                                        locations,
                                        warehouses,
                                    )
                                    value=value.get_untracked()
                                />
                            }
                        })}
                    </Transition>
                </CollapsibleCard>
            </div>
        }
        .into_any()
    })
    .icon(Icon::SlidersHorizontal);

    let history_tab = Tab::new("history", "History", move || {
        view! { <RecordHistory kind=kinds::STOCK_LOCATION id=Some(location_id.to_string()) /> }
            .into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader
            title=title
            icon=Icon::Boxes
            back=("/inventory/locations", l!("locations.title"))
        >
            <div class="flex flex-wrap items-center gap-1.5">
                <Badge label=kind tone=Tone::Brand />
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="stock-location" tabs=vec![details_tab, history_tab] />
    }
}
