//! Inventory: what the workspace stocks, where it is, and how it is valued.
//!
//! ```text
//! /inventory                      the app's home
//! /inventory/items                the list
//! /inventory/items/new            a form
//! /inventory/items/:id            Details | Variants | Pictures | Accounting | History
//! /inventory/categories           the list, drawn as a tree
//! /inventory/categories/new       a form
//! /inventory/categories/:id       Details | History
//! /inventory/warehouses           the list
//! /inventory/warehouses/new       a form
//! /inventory/warehouses/:id       Details | History
//! /inventory/locations            the list, drawn as a tree
//! /inventory/locations/new        a form
//! /inventory/locations/:id        Details | History
//! /inventory/units                the list
//! /inventory/units/new            a form
//! /inventory/units/:id            Details | History
//! ```
//!
//! The four screens under items are setup: a workspace touches categories,
//! warehouses, locations and units when it starts and rarely afterwards. Items
//! is the one somebody has open all day, which is why it is the only screen
//! here that grew tabs.

pub mod home;
pub mod item;
pub mod item_categories;
pub mod items;
pub mod pictures;
pub mod stock_locations;
pub mod units;
pub mod variants;
pub mod warehouses;
