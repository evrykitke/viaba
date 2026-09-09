//! The people area's three list screens.
//!
//! Thin by design - each is a `PageHeader` over a grid whose whole shape lives
//! in `ui::table::config`. The documents behind them are their own files,
//! because those are where the decisions are.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::ui::table::DataGrid;
use crate::ui::table::config::employees::employees_grid;
use crate::ui::table::config::job_positions::job_positions_grid;
use crate::ui::table::config::work_locations::work_locations_grid;

/// Who works here. Leavers included - see the grid's own docs.
#[component]
pub fn employees_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("employees.title")) />

        <PageHeader
            title=l!("employees.title")
            subtitle=l!("employees.subtitle")
            icon=Icon::Users
        />

        <DataGrid config=employees_grid() />
    }
}

/// What the organization is made of, whether or not anybody is doing it.
#[component]
pub fn job_positions_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("job_positions.title")) />

        <PageHeader
            title=l!("job_positions.title")
            subtitle=l!("job_positions.subtitle")
            icon=Icon::ListChecks
        />

        <DataGrid config=job_positions_grid() />
    }
}

#[component]
pub fn work_locations_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("work_locations.title")) />

        <PageHeader
            title=l!("work_locations.title")
            subtitle=l!("work_locations.subtitle")
            icon=Icon::Warehouse
        />

        <DataGrid config=work_locations_grid() />
    }
}
