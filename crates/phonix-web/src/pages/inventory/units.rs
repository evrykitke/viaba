//! Units of measure: the list, the one that adds one, and the one that edits.
//!
//! Three components in one file rather than the two-file split the department
//! screens use. These are setup screens somebody opens twice a year, and each
//! component is short enough that splitting them across files would cost a
//! reader more than it explained.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_inventory::unit::UnitInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{list_units, unit_edit};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::units::unit_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::units::units_grid;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn units_page() -> impl IntoView {
    view! {
        // "Phonix" is the product's name, not a word.
        <Title text=format!("{} | Phonix", l!("units.title")) />

        <PageHeader
            title=l!("units.title")
            subtitle=l!("units.subtitle")
            icon=Icon::Ruler
        />

        <DataGrid config=units_grid() />
    }
}

/// Adding a unit.
#[component]
pub fn unit_new_page() -> impl IntoView {
    // The existing units, so the help text under the factor can name the base
    // of whichever class is chosen.
    let units = Resource::new(|| (), |()| async move { list_units().await });

    view! {
        <Title text=format!("{} | Phonix", l!("units.new")) />

        <PageHeader
            title=l!("units.new")
            subtitle=l!("units.new.subtitle")
            icon=Icon::Ruler
            back=("/inventory/units", l!("units.title"))
        />

        // A single-column form, so the card ends where it ends rather than
        // stretching four short fields across a wide monitor.
        <div class="max-w-3xl">
            <CollapsibleCard open=true>
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        // A failed fetch costs the help text, not the screen.
                        let units = units.await.unwrap_or_default();

                        view! {
                            <EntityForm config=unit_form(units) value=UnitInput::blank() />
                        }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}

/// One unit: what it is, and what has been done to it.
#[component]
pub fn unit_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let unit_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(unit_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => unit_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not a unit id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.unit_of_measure.singular")) />

        // Transition, not Suspense: moving between units re-suspends, and a
        // fallback would blank the screen somebody is looking at.
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <UnitEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.unit_of_measure.singular")
                                    icon=Icon::Ruler
                                    back=("/inventory/units", l!("units.title"))
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
fn unit_editor(draft: UnitInput) -> impl IntoView {
    let unit_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = format!("{} · {}", draft.code, draft.name);
    let is_active = draft.is_active;

    // Hoisted above the tab strip: a tab's render closure runs again each time
    // it comes back on screen.
    let units = Resource::new(|| (), |()| async move { list_units().await });
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", "Details", move || {
        view! {
            <div class="max-w-3xl">
                <CollapsibleCard open=true>
                    <Transition fallback=|| {
                        view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                    }>
                        {move || Suspend::new(async move {
                            let units = units.await.unwrap_or_default();

                            view! {
                                <EntityForm
                                    config=unit_form(units)
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
        view! { <RecordHistory kind=kinds::UNIT_OF_MEASURE id=Some(unit_id.to_string()) /> }
            .into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader title=title icon=Icon::Ruler back=("/inventory/units", l!("units.title"))>
            <div class="flex flex-wrap items-center gap-1.5">
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="unit" tabs=vec![details_tab, history_tab] />
    }
}
