//! The receipt behind a payment.
//!
//! Its address is under the payment rather than under the reports, because it
//! is that record drawn as a document and a link to it is a link to the
//! payment. What the receipt *is* lives in
//! [`ui::report::config::receipt`](crate::ui::report::config::receipt).

use leptos::prelude::*;
use leptos_meta::Title;
use uuid::Uuid;

use crate::components::page::Panel;
use crate::l;
use crate::server_fns::books_fns::payment_detail;
use crate::ui::report::config::receipt::receipt as definition;
use crate::ui::report::{Report, ReportViewer};

#[component]
pub fn payment_receipt_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let payment_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let payment = Resource::new(payment_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => payment_detail(id).await.ok(),
            Err(_) => None,
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("payments.receipt")) />

        <ReportViewer
            definition=definition()
            back=("/selling/payments", l!("entity.payment.plural"))
        >
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    match payment.await {
                        Some(payment) => {
                            view! { <Report definition=definition() data=payment /> }.into_any()
                        }
                        None => {
                            view! {
                                <Panel>
                                    <p class="py-6 text-center text-sm text-content-muted">
                                        {l!("not_found.heading")}
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
