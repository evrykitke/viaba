//! Adjustment types: the list, the one that adds one, and the one that edits.
//!
//! Three components in one file, as the units screens are. These are setup
//! screens somebody opens twice a year, and splitting them across files would
//! cost a reader more than it explained.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_inventory::adjustment::AdjustmentTypeInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{adjustment_type_edit, postable_accounts};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::adjustment_types::adjustment_type_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::adjustment_types::adjustment_types_grid;
use crate::ui::tabs::{Tab, TabbedPanel};

const BACK: &str = "/inventory/adjustment-types";

#[component]
pub fn adjustment_types_page() -> impl IntoView {
    view! {
        // "Phonix" is the product's name, not a word.
        <Title text=format!("{} | Phonix", l!("adjustment_types.title")) />

        <PageHeader
            title=l!("adjustment_types.title")
            subtitle=l!("adjustment_types.subtitle")
            icon=Icon::SlidersHorizontal
        />

        <DataGrid config=adjustment_types_grid() />
    }
}

/// Adding a reason.
#[component]
pub fn adjustment_type_new_page() -> impl IntoView {
    let chart = Resource::new(|| (), |()| async move { postable_accounts().await });

    view! {
        <Title text=format!("{} | Phonix", l!("adjustment_types.new")) />

        <PageHeader
            title=l!("adjustment_types.new")
            subtitle=l!("adjustment_types.new.subtitle")
            icon=Icon::SlidersHorizontal
            back=(BACK, l!("adjustment_types.title"))
        />

        // A single-column form, so the card ends where it ends rather than
        // stretching six short fields across a wide monitor.
        <div class="max-w-3xl">
            <CollapsibleCard open=true>
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        // No ledger, or none this caller may read: the picker
                        // is empty and the type takes the workspace default,
                        // which is what it would have done anyway.
                        let chart = chart.await.unwrap_or_default();

                        view! {
                            <EntityForm
                                config=adjustment_type_form(chart)
                                value=AdjustmentTypeInput::blank()
                            />
                        }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}

/// One reason: what it is, and what has been done to it.
#[component]
pub fn adjustment_type_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let type_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(type_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => adjustment_type_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not an adjustment type id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.adjustment_type.singular")) />

        // Transition, not Suspense: moving between types re-suspends, and a
        // fallback would blank the screen somebody is looking at.
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <AdjustmentTypeEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.adjustment_type.singular")
                                    icon=Icon::SlidersHorizontal
                                    back=(BACK, l!("adjustment_types.title"))
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
fn adjustment_type_editor(draft: AdjustmentTypeInput) -> impl IntoView {
    let type_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = format!("{} · {}", draft.code, draft.name);
    let is_active = draft.is_active;
    let is_system = draft.is_system;

    // Hoisted above the tab strip: a tab's render closure runs again each time
    // it comes back on screen.
    let chart = Resource::new(|| (), |()| async move { postable_accounts().await });
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", l!("common.details"), move || {
        view! {
            <div class="max-w-3xl">
                <CollapsibleCard open=true>
                    <Transition fallback=|| {
                        view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                    }>
                        {move || Suspend::new(async move {
                            let chart = chart.await.unwrap_or_default();

                            view! {
                                <EntityForm
                                    config=adjustment_type_form(chart)
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

    let history_tab = Tab::new("history", l!("common.history"), move || {
        view! { <RecordHistory kind=kinds::ADJUSTMENT_TYPE id=Some(type_id.to_string()) /> }
            .into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader
            title=title
            icon=Icon::SlidersHorizontal
            back=(BACK, l!("adjustment_types.title"))
        >
            <div class="flex flex-wrap items-center gap-1.5">
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
                {is_system.then(|| view! { <Badge label=l!("adjustment_types.seeded") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="adjustment-type" tabs=vec![details_tab, history_tab] />
    }
}
