//! Inventory's front page.
//!
//! # Two numbers, and the gap between them is the interesting one
//!
//! Items, and the ones a quantity is kept for. A workspace where the two are
//! equal is counting its services and its screws by the tub, which is work
//! nobody does twice; a workspace where the second is zero has a catalogue
//! rather than a stockroom.
//!
//! Neither is a warning. It is a fact about what the workspace sells, shown
//! where somebody arranging it will see it.

use leptos::prelude::*;
use phonix_core::apps;
use phonix_core::i18n::Message;
use phonix_core::permissions;

use crate::components::app_home::{AppHome, Shortcut, Stat};
use crate::i18n::t;
use crate::icons::Icon;
use crate::server_fns::inventory_fns::inventory_counts;

#[component]
pub fn inventory_home_page() -> impl IntoView {
    let counts = Resource::new(|| (), |()| async move { inventory_counts().await.ok() });

    let stats = Signal::derive(move || {
        let Some(Some((total, tracked))) = counts.get() else {
            return Vec::new();
        };

        vec![
            Stat::new(t(&Message::new("inventory.home.items")), total),
            Stat::new(t(&Message::new("inventory.home.tracked_items")), tracked),
        ]
    });

    #[allow(
        clippy::expect_used,
        reason = "the catalog is a compiled constant and this app is in it"
    )]
    let app = apps::find(apps::INVENTORY).expect("inventory is in the catalog");

    view! {
        <AppHome
            app=app
            stats=stats
            shortcuts=vec![
                Shortcut::new(
                    t(&Message::new("inventory.home.items")),
                    t(&Message::new("inventory.home.items_detail")),
                    "/inventory/items",
                    Icon::Package,
                )
                .require(permissions::ITEMS)
                .primary(),
                Shortcut::new(
                    t(&Message::new("inventory.home.categories")),
                    t(&Message::new("inventory.home.categories_detail")),
                    "/inventory/categories",
                    Icon::ListTree,
                )
                .require(permissions::ITEM_CATEGORIES),
                Shortcut::new(
                    t(&Message::new("inventory.home.warehouses")),
                    t(&Message::new("inventory.home.warehouses_detail")),
                    "/inventory/warehouses",
                    Icon::Warehouse,
                )
                .require(permissions::WAREHOUSES),
                Shortcut::new(
                    t(&Message::new("inventory.home.locations")),
                    t(&Message::new("inventory.home.locations_detail")),
                    "/inventory/locations",
                    Icon::Boxes,
                )
                .require(permissions::STOCK_LOCATIONS),
                Shortcut::new(
                    t(&Message::new("inventory.home.units")),
                    t(&Message::new("inventory.home.units_detail")),
                    "/inventory/units",
                    Icon::Ruler,
                )
                .require(permissions::UNITS),
            ]
        />
    }
}
