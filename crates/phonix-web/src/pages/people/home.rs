//! People's front page.
//!
//! # Two numbers, and the gap between them is the interesting one
//!
//! Departments, and the ones that are chargeable. A workspace where the two are
//! equal has marked every department as a cost centre, which is almost always
//! wrong: the divisions above the teams should be groupings, because posting to
//! a parent and its children is how a report counts the same money twice.
//!
//! Neither number is a warning, and neither should be. It is a fact about the
//! organization, shown where somebody arranging it will see it.

use leptos::prelude::*;
use phonix_core::apps;
use phonix_core::i18n::Message;
use phonix_core::permissions;

use crate::components::app_home::{AppHome, Shortcut, Stat};
use crate::i18n::t;
use crate::icons::Icon;
use crate::server_fns::hr_fns::department_counts;

#[component]
pub fn people_home_page() -> impl IntoView {
    let counts = Resource::new(|| (), |()| async move { department_counts().await.ok() });

    let stats = Signal::derive(move || {
        let Some(Some((total, chargeable))) = counts.get() else {
            return Vec::new();
        };

        vec![
            Stat::new(t(&Message::new("hr.home.departments")), total),
            Stat::new(t(&Message::new("hr.home.cost_centres")), chargeable),
        ]
    });

    #[allow(
        clippy::expect_used,
        reason = "the catalog is a compiled constant and this app is in it"
    )]
    let app = apps::find(apps::HR).expect("hr is in the catalog");

    view! {
        <AppHome
            app=app
            stats=stats
            shortcuts=vec![
                Shortcut::new(
                    t(&Message::new("hr.home.departments")),
                    t(&Message::new("hr.home.departments_detail")),
                    "/people/departments",
                    Icon::Building2,
                )
                .require(permissions::DEPARTMENTS)
                .primary(),
            ]
        />
    }
}
