//! The item list, and the screen that adds one.
//!
//! Adding one is a page rather than a dialog for the reason the party screen is
//! a page: what happens next is a second step - the variants, the pictures, the
//! accounts - and a dialog that closes onto a list has nowhere to send somebody.

use leptos::prelude::*;
use leptos_meta::Title;

use app_inventory::item::ItemInput;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{selectable_categories, selectable_units};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::items::item_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::items::items_grid;

#[component]
pub fn items_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("items.title")) />

        <PageHeader title=l!("items.title") subtitle=l!("items.subtitle") icon=Icon::Package />

        <DataGrid config=items_grid() />
    }
}

/// Adding an item.
#[component]
pub fn item_new_page() -> impl IntoView {
    // Two resources rather than one call that fetches both: a resource starts
    // as soon as it is created, so these are already in flight together.
    let categories = Resource::new(|| (), |()| async move { selectable_categories().await });
    let units = Resource::new(|| (), |()| async move { selectable_units().await });

    view! {
        <Title text=format!("{} | Phonix", l!("items.new")) />

        <PageHeader
            title=l!("items.new")
            subtitle=l!("items.new.subtitle")
            icon=Icon::Package
            back=("/inventory/items", l!("items.title"))
        />

        // Headless, and no measure: the heading above already says what this
        // is, and the form asks for the whole width.
        <div>
            <CollapsibleCard open=true>
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        // Both pickers are required fields, so an empty one is a
                        // form that cannot be saved - but the save says which,
                        // and a failed screen would say nothing at all.
                        let categories = categories.await.unwrap_or_default();
                        let units = units.await.unwrap_or_default();

                        view! {
                            <EntityForm
                                config=item_form(categories, units)
                                value=ItemInput::blank()
                            />
                        }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}
