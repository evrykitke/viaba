//! Item categories: the list, adding one, and editing one.
//!
//! An accounting screen wearing an inventory heading. What is decided here -
//! how stock is costed, whether a movement posts a journal as it happens, and
//! which units a pick reaches for - is what the workspace's stock is worth.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_inventory::category::CategoryInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{item_category_edit, list_item_categories};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::item_categories::item_category_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::item_categories::item_categories_grid;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn item_categories_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("categories.title")) />

        <PageHeader
            title=l!("categories.title")
            subtitle=l!("categories.subtitle")
            icon=Icon::ListTree
        />

        <DataGrid config=item_categories_grid() />
    }
}

/// Adding a category.
#[component]
pub fn item_category_new_page() -> impl IntoView {
    let categories = Resource::new(|| (), |()| async move { list_item_categories().await });

    view! {
        <Title text=format!("{} | Phonix", l!("categories.new")) />

        <PageHeader
            title=l!("categories.new")
            subtitle=l!("categories.new.subtitle")
            icon=Icon::ListTree
            back=("/inventory/categories", l!("categories.title"))
        />

        <div class="max-w-3xl">
            <CollapsibleCard open=true>
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        let categories = categories.await.unwrap_or_default();

                        view! {
                            <EntityForm
                                config=item_category_form(None, categories)
                                value=CategoryInput::blank()
                            />
                        }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}

/// One category: what it decides, and what has been done to it.
#[component]
pub fn item_category_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let category_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(category_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => item_category_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not a category id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.item_category.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <ItemCategoryEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.item_category.singular")
                                    icon=Icon::ListTree
                                    back=("/inventory/categories", l!("categories.title"))
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
fn item_category_editor(draft: CategoryInput) -> impl IntoView {
    let category_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = draft.name.clone();
    let costing = crate::i18n::t(&draft.costing_method.label());
    let is_active = draft.is_active;

    let categories = Resource::new(|| (), |()| async move { list_item_categories().await });
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", "Details", move || {
        view! {
            <div class="max-w-3xl">
                <CollapsibleCard open=true>
                    <Transition fallback=|| {
                        view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                    }>
                        {move || Suspend::new(async move {
                            let categories = categories.await.unwrap_or_default();

                            view! {
                                <EntityForm
                                    config=item_category_form(Some(category_id), categories)
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
        view! { <RecordHistory kind=kinds::ITEM_CATEGORY id=Some(category_id.to_string()) /> }
            .into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader
            title=title
            icon=Icon::ListTree
            back=("/inventory/categories", l!("categories.title"))
        >
            <div class="flex flex-wrap items-center gap-1.5">
                <Badge label=costing tone=Tone::Brand />
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="item-category" tabs=vec![details_tab, history_tab] />
    }
}
