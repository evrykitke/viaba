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
//! /inventory/requisitions         what departments have asked for
//! /inventory/requisitions/new     a form
//! /inventory/requisitions/:id     the editor while draft, the document after
//! /inventory/orders               what has been ordered, and what is owed
//! /inventory/orders/new           a form
//! /inventory/orders/:id           the editor while draft, the document after
//! /inventory/bills                what suppliers charged, and how it matched
//! /inventory/bills/new            a form. `?order=<id>` bills what one owes
//! /inventory/bills/:id            the editor while draft, the document after
//! /inventory/unbilled             received not billed, by age
//! /inventory/landed-costs         freight and duty, and where it landed
//! /inventory/landed-costs/new     a form. `?receipt=<id>` opens it against one
//! /inventory/landed-costs/:id     the editor while draft, the document after
//! /inventory/receipts             what arrived, and what it was worth
//! /inventory/receipts/new         a form. `?order=<id>` opens it against one
//! /inventory/receipts/:id         the tally while draft, the document after
//! /inventory/transfers            stock moved between our own places
//! /inventory/transfers/new        a form
//! /inventory/transfers/:id        the editor while draft, the document after
//! /inventory/stock                what is on hand, and where
//! /inventory/moves                every change to every quantity
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

pub mod bill;
pub mod consolidation;
pub mod home;
pub mod item;
pub mod item_categories;
pub mod items;
pub mod landed_cost;
pub mod pictures;
pub mod receipt;
pub mod requisition;
pub mod procurement;
pub mod purchase_order;
pub mod stock;
pub mod transfer;
pub mod stock_locations;
pub mod units;
pub mod variants;
pub mod warehouses;
