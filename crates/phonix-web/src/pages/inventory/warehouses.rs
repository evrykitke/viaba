//! Warehouses: the list, adding one, and editing one.
//!
//! There is no delete. A warehouse owns the locations that carry every movement
//! that ever crossed them, so switching it off is the operation offered.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_inventory::warehouse::WarehouseInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::warehouse_edit;
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::warehouses::warehouse_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::warehouses::warehouses_grid;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn warehouses_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("warehouses.title")) />

        <PageHeader
            title=l!("warehouses.title")
            subtitle=l!("warehouses.subtitle")
            icon=Icon::Warehouse
        />

        <DataGrid config=warehouses_grid() />
    }
}

/// Adding a warehouse.
///
/// A page rather than a dialog, because saving it creates a small tree of
/// locations and the next thing somebody does is look at them.
#[component]
pub fn warehouse_new_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("warehouses.new")) />

        <PageHeader
            title=l!("warehouses.new")
            subtitle=l!("warehouses.new.subtitle")
            icon=Icon::Warehouse
            back=("/inventory/warehouses", l!("warehouses.title"))
        />

        <div class="max-w-3xl">
            <CollapsibleCard
                title=l!("warehouses.new")
                detail=l!("warehouses.new.subtitle")
                icon=Icon::Warehouse
                open=true
            >
                <EntityForm config=warehouse_form() value=WarehouseInput::blank() />
            </CollapsibleCard>
        </div>
    }
}

/// One warehouse: how it works, and what has been done to it.
#[component]
pub fn warehouse_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let warehouse_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(warehouse_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => warehouse_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not a warehouse id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.warehouse.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <WarehouseEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.warehouse.singular")
                                    icon=Icon::Warehouse
                                    back=("/inventory/warehouses", l!("warehouses.title"))
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
fn warehouse_editor(draft: WarehouseInput) -> impl IntoView {
    let warehouse_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = format!("{} · {}", draft.code, draft.name);
    let is_active = draft.is_active;
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", "Details", move || {
        view! {
            <div class="max-w-3xl">
                <CollapsibleCard title=l!("warehouses.edit") icon=Icon::Warehouse open=true>
                    <EntityForm config=warehouse_form() value=value.get_untracked() />
                </CollapsibleCard>
            </div>
        }
        .into_any()
    })
    .icon(Icon::SlidersHorizontal);

    let history_tab = Tab::new("history", "History", move || {
        view! { <RecordHistory kind=kinds::WAREHOUSE id=Some(warehouse_id.to_string()) /> }
            .into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader
            title=title
            icon=Icon::Warehouse
            back=("/inventory/warehouses", l!("warehouses.title"))
        >
            <div class="flex flex-wrap items-center gap-1.5">
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="warehouse" tabs=vec![details_tab, history_tab] />
    }
}
