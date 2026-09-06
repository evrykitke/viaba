//! One account: what it is, and what has been done to it.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_books::account::Account;

use crate::components::account_class::ClassChip;
use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::i18n::t;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::account_detail;
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::accounts::account_form;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn account_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let account_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let account = Resource::new(account_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => account_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not an account id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.account.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match account.await {
                    Ok(account) => view! { <AccountEditor account=account /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.account.singular")
                                    icon=Icon::ListTree
                                    back=("/sales/accounts", l!("accounts.title"))
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
fn account_editor(account: Account) -> impl IntoView {
    let account_id = account.id;
    let title = account.label();
    let class = account.class();
    let normal_balance = t(&account.normal_balance().label());
    let type_label = t(&account.account_type.label());
    let description = account.description.clone();
    let postable = account.is_postable();
    let is_active = account.is_active;
    let is_default = account.is_default;
    let draft = app_books::account::AccountInput::from_account(&account);

    let details_tab = Tab::new("details", "Details", move || {
        let description = description.clone();
        let type_label = type_label.clone();
        let normal_balance = normal_balance.clone();
        let draft = draft.clone();

        view! {
            <div class="grid gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                <CollapsibleCard title=l!("accounts.edit") icon=Icon::ListTree open=true>
                    <EntityForm config=account_form(true, Vec::new()) value=draft />
                </CollapsibleCard>

                // What the software concludes from the type, rather than
                // anything the person typed. Shown beside the form because
                // "which side does this land on" is the question the type
                // picker is really asking.
                <div class="space-y-2 rounded-card border border-edge bg-surface-raised p-4">
                    <h2 class="text-xs font-medium uppercase tracking-wide text-content-subtle">
                        {l!("accounts.classification")}
                    </h2>
                    <dl class="space-y-2 text-sm">
                        <Fact label=l!("accounts.class") value=(move || view! { <ClassChip class=class /> }).into_any() />
                        <Fact label=l!("field.type") value=type_label.into_any() />
                        <Fact label=l!("accounts.normal_balance") value=normal_balance.into_any() />
                        <Fact
                            label=l!("accounts.postable")
                            value=if postable {
                                l!("accounts.postable.yes")
                            } else {
                                l!("accounts.postable.no")
                            }
                                .into_any()
                        />
                    </dl>
                    {description
                        .map(|description| {
                            view! {
                                <p class="border-t border-edge pt-2 text-xs leading-relaxed text-content-muted">
                                    {description}
                                </p>
                            }
                        })}
                </div>
            </div>
        }
        .into_any()
    })
    .icon(Icon::SlidersHorizontal);

    let history_tab = Tab::new("history", "History", move || {
        view! { <RecordHistory kind=kinds::ACCOUNT id=Some(account_id.to_string()) /> }.into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader title=title icon=Icon::ListTree back=("/sales/accounts", l!("accounts.title"))>
            <div class="flex flex-wrap items-center gap-1.5">
                <ClassChip class=class />
                {(!postable).then(|| view! { <Badge label=l!("accounts.postable.no") /> })}
                {is_default.then(|| view! { <Badge label=l!("accounts.default") /> })}
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="account" tabs=vec![details_tab, history_tab] />
    }
}

/// One labelled fact.
#[component]
fn fact(label: String, value: AnyView) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between gap-3">
            <dt class="text-xs text-content-subtle">{label}</dt>
            <dd class="text-right text-content">{value}</dd>
        </div>
    }
}
