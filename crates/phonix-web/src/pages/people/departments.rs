//! The department list, and the screen that adds one.

use leptos::prelude::*;
use leptos_meta::Title;

use app_hr::department::DepartmentInput;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{list_departments, manager_candidates};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::departments::department_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::departments::departments_grid;

#[component]
pub fn departments_page() -> impl IntoView {
    view! {
        // "Phonix" is the product's name, not a word.
        <Title text=format!("{} | Phonix", l!("departments.title")) />

        <PageHeader
            title=l!("departments.title")
            subtitle=l!("departments.subtitle")
            icon=Icon::Building2
        />

        <DataGrid config=departments_grid() />
    }
}

/// Adding a department.
///
/// A page rather than a dialog over the list, for the reason the party screen
/// is: what happens next is usually a second step - the teams that go inside
/// it - and a dialog that closes onto a list has nowhere to send somebody.
#[component]
pub fn department_new_page() -> impl IntoView {
    // Two resources rather than one call that fetches both: a resource starts
    // as soon as it is created, so these are already in flight together and
    // awaiting them in the same block is the join.
    let departments = Resource::new(|| (), |()| async move { list_departments().await });
    let managers = Resource::new(|| (), |()| async move { manager_candidates().await });

    view! {
        <Title text=format!("{} | Phonix", l!("departments.new")) />

        <PageHeader
            title=l!("departments.new")
            subtitle=l!("departments.new.subtitle")
            icon=Icon::Building2
            back=("/people/departments", l!("departments.title"))
        />

        // A single-column form, so the card ends where it ends rather than
        // stretching a name across a wide monitor.
        <div class="max-w-3xl">
            // `open`, because this card *is* the page. Arriving closed would be
            // a screen with a heading and nothing to do on it - the one case
            // the component's own documentation names for this prop.
            <CollapsibleCard
                title=l!("departments.new")
                detail=l!("departments.new.subtitle")
                icon=Icon::Building2
                open=true
            >
                // Transition rather than Suspense: the fallback replaces
                // nothing here, but navigating away and back must not blank a
                // form somebody is filling in.
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        // A failed fetch is an empty picker rather than a failed
                        // screen. Both are optional fields: a department with no
                        // parent is top level and one with no manager is
                        // ordinary, so refusing to let anybody add one because
                        // the user list would not load is the wrong trade.
                        let departments = departments.await.unwrap_or_default();
                        let managers = managers.await.unwrap_or_default();

                        view! {
                            <EntityForm
                                config=department_form(None, departments, managers)
                                value=DepartmentInput::blank()
                            />
                        }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}
