//! The product list, on paper.
//!
//! The screen is the fetch; what the list *is* lives in
//! [`ui::report::config::product_list`](crate::ui::report::config::product_list).

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::query::PageRequest;

use crate::components::page::Panel;
use crate::l;
use crate::server_fns::inventory_fns::list_items;
use crate::ui::report::config::product_list::{ROWS_PER_RUN, product_list};
use crate::ui::report::{Report, ReportViewer};

#[component]
pub fn item_report_page() -> impl IntoView {
    // One bounded page of the read that already exists. A report that asked
    // for everything would be the unpaged read this codebase keeps undoing.
    let items = Resource::new(
        || (),
        |()| async move {
            list_items(PageRequest {
                page: 1,
                per_page: ROWS_PER_RUN,
                ..PageRequest::default()
            })
            .await
            .ok()
        },
    );

    view! {
        <Title text=format!("{} | Phonix", l!("items.title")) />

        <ReportViewer
            definition=product_list()
            back=("/inventory/items", l!("items.title"))
        >
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    match items.await {
                        Some(page) => {
                            view! { <Report definition=product_list() data=page /> }.into_any()
                        }
                        None => {
                            view! {
                                <Panel>
                                    <p class="py-6 text-center text-sm text-content-muted">
                                        {l!("grid.empty.title")}
                                    </p>
                                </Panel>
                            }
                                .into_any()
                        }
                    }
                })}
            </Transition>
        </ReportViewer>
    }
}
