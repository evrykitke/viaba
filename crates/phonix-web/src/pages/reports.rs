//! Where a report is found.
//!
//! One list of everything the engine serves, grouped by the app that declares
//! it. What is on it is what this viewer may run: the index reads the same
//! entries the exporter does, so a report cannot be reachable one way and not
//! the other.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::components::A;
use phonix_core::apps::{AppDescriptor, CATALOG};

use phonix_core::i18n::Message;

use crate::components::page::{PageHeader, Panel, Section};
use crate::i18n::t;
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::reports::{SERVER_REPORTS, ServerReport};
use crate::ui::viewer::Viewer;

#[component]
pub fn reports_page() -> impl IntoView {
    let viewer = Viewer::get();

    // A gate here is a row or nothing, so it is read inside a boundary: the
    // session is a resource, and a list drawn before it resolves would hydrate
    // to a different one.
    let listed = move || {
        let user = viewer.get();

        CATALOG
            .iter()
            .filter_map(|app| {
                let reports: Vec<&ServerReport> = SERVER_REPORTS
                    .iter()
                    .filter(|report| {
                        report.app().is_some_and(|owner| owner.id == app.id)
                            && user
                                .as_ref()
                                .is_some_and(|user| user.can(report.permission))
                    })
                    .collect();

                (!reports.is_empty()).then_some((app, reports))
            })
            .collect::<Vec<(&AppDescriptor, Vec<&ServerReport>)>>()
    };

    view! {
        <Title text=format!("{} | Phonix", l!("reports.index.title")) />

        <PageHeader
            title=l!("reports.index.title")
            subtitle=l!("reports.index.subtitle")
            icon=Icon::ChartColumn
        />

        <Suspense fallback=|| ()>
            {move || {
                let listed = listed();

                if listed.is_empty() {
                    return view! {
                        <Panel>
                            <p class="py-6 text-center text-sm text-content-muted">
                                {l!("reports.index.empty")}
                            </p>
                        </Panel>
                    }
                        .into_any();
                }

                view! {
                    <Panel>
                        {listed
                            .into_iter()
                            .map(|(app, reports)| {
                                view! {
                                    <Section title=t(&Message::new(app.name))>
                                        <ul class="space-y-1">
                                            {reports
                                                .into_iter()
                                                .map(|report| {
                                                    view! {
                                                        <li>
                                                            <A
                                                                href=report.href
                                                                attr:class="flex items-center gap-2 rounded-control px-2 py-1.5 text-sm text-content hover:bg-surface-hover"
                                                            >
                                                                <Icon
                                                                    icon=Icon::FileText
                                                                    size=IconSize::Xs
                                                                />
                                                                {t(&Message::new(report.title))}
                                                            </A>
                                                        </li>
                                                    }
                                                })
                                                .collect_view()}
                                        </ul>
                                    </Section>
                                }
                            })
                            .collect_view()}
                    </Panel>
                }
                    .into_any()
            }}
        </Suspense>
    }
}
