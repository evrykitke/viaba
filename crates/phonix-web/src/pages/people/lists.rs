//! The people area's seven list screens.
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
use crate::ui::table::config::applicants::applicants_grid;
use crate::ui::table::config::employees::employees_grid;
use crate::ui::table::config::holiday_lists::holiday_lists_grid;
use crate::ui::table::config::job_positions::job_positions_grid;
use crate::ui::table::config::movements::movements_grid;
use crate::ui::table::config::shift_types::shift_types_grid;
use crate::ui::table::config::work_locations::work_locations_grid;

/// Who works here. Leavers included - see the grid's own docs.
#[component]
pub fn employees_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("employees.title")) />

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
        <Title text=format!("{} | Evrykit", l!("job_positions.title")) />

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
        <Title text=format!("{} | Evrykit", l!("work_locations.title")) />

        <PageHeader
            title=l!("work_locations.title")
            subtitle=l!("work_locations.subtitle")
            icon=Icon::Warehouse
        />

        <DataGrid config=work_locations_grid() />
    }
}

/// The days nobody is expected to work, per region and year.
#[component]
pub fn holiday_lists_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("holidays.title")) />

        <PageHeader
            title=l!("holidays.title")
            subtitle=l!("holidays.subtitle")
            icon=Icon::Calendar
        />

        <DataGrid config=holiday_lists_grid() />
    }
}

/// What people are expected to work, and what counts as on time.
#[component]
pub fn shift_types_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("shifts.title")) />

        <PageHeader
            title=l!("shifts.title")
            subtitle=l!("shifts.subtitle")
            icon=Icon::Clock
        />

        <DataGrid config=shift_types_grid() />
    }
}

/// What has happened to people: promotions, transfers and exits.
#[component]
pub fn movements_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("movements.title")) />

        <PageHeader
            title=l!("movements.title")
            subtitle=l!("movements.subtitle")
            icon=Icon::ArrowRight
        />

        <DataGrid config=movements_grid() />
    }
}

/// Who wants the jobs this workspace has.
#[component]
pub fn applicants_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Evrykit", l!("applicants.title")) />

        <PageHeader
            title=l!("applicants.title")
            subtitle=l!("applicants.subtitle")
            icon=Icon::UserPlus
        />

        <DataGrid config=applicants_grid() />
    }
}
