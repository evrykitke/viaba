//! The item box on a document line.
//!
//! # Why a document line does not use a `SelectField`
//!
//! Because it cannot hold the catalogue. Every purchase order, receipt, bill,
//! transfer, requisition and consolidation screen loaded *every purchasable
//! variant in the workspace* and rendered a dropdown over it. That is a page
//! that works beautifully on a demo of forty items and stops loading somewhere
//! north of a few thousand - and a wholesaler has a few hundred thousand. The
//! browser was being asked to hold, filter and diff a table it has no business
//! knowing.
//!
//! So the search happens where the index is. This is a
//! [`LookupField`] over [`Choices::Live`]: what has been typed goes to
//! `find_variants`, which answers with fifty rows, and the panel shows them.
//!
//! # What it answers with
//!
//! The whole [`VariantChoice`], not an id. A line that has just been given an
//! item wants its description and its purchase unit as well, and every caller
//! would otherwise look the row up again in a list it no longer holds.
//!
//! # The chip it opens with
//!
//! An existing line already carries its own description, and that is what the
//! field shows before anything is searched - so opening a fifty-line receipt
//! costs no queries at all. Pass it as `initial`; the label is the line's
//! words, not a fresh lookup's.

use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use uuid::Uuid;

use crate::server_fns::inventory_fns::find_variants;
use crate::ui::form::field::Choice;
use crate::ui::lookup::{Choices, LookupField};

/// A searchable item box, backed by the database rather than by the page.
#[component]
pub fn item_lookup(
    /// What the line already names, if it names anything: the variant's id and
    /// the words already on the line.
    #[prop(optional_no_strip)]
    initial: Option<Choice>,
    /// Run when the choice changes. `None` means it was cleared.
    on_pick: Callback<Option<VariantChoice>>,
    #[prop(optional, into)] disabled: Signal<bool>,
    /// Ties a `<label for>` to the field, the way every other control here
    /// takes one.
    #[prop(optional, into)]
    id: String,
    #[prop(optional, into)] invalid: Signal<bool>,
    #[prop(optional_no_strip)] placeholder: Option<String>,
) -> impl IntoView {
    let selected = RwSignal::new(initial.into_iter().collect::<Vec<_>>());
    let found = RwSignal::new(Vec::<VariantChoice>::new());

    // Which question this answer belongs to. Typing "wid", "widg", "widge"
    // starts three requests and they can come back in any order; without this,
    // the slowest one wins and the panel shows the results for a prefix of
    // what is in the box.
    let asked = StoredValue::new(0_u32);

    let on_query = Callback::new(move |needle: String| {
        let Some(mine) = asked.try_update_value(|count| {
            *count += 1;
            *count
        }) else {
            return;
        };

        leptos::task::spawn_local(async move {
            let Ok(rows) = find_variants(needle).await else {
                return;
            };

            if asked.try_get_value() == Some(mine) {
                let _ = found.try_set(rows);
            }
        });
    });

    let choices = Signal::derive(move || {
        found
            .try_get()
            .unwrap_or_default()
            .into_iter()
            .map(|variant| {
                Choice::new(variant.id.to_string(), variant.label()).detail(variant.code)
            })
            .collect::<Vec<_>>()
    });

    // `LookupField` answers by writing into `selected`, so this is where a
    // pick is noticed. The closure's argument is what it returned last time,
    // which is `None` on the first run - and the first run is the field being
    // built from `initial`, not somebody choosing something.
    Effect::new(move |previous: Option<Option<Uuid>>| {
        let now = selected
            .try_get()
            .unwrap_or_default()
            .first()
            .and_then(|choice| choice.value.parse::<Uuid>().ok());

        if let Some(before) = previous
            && before != now
        {
            let picked = now.and_then(|id| {
                found.with_untracked(|rows| rows.iter().find(|row| row.id == id).cloned())
            });

            let _ = on_pick.try_run(picked);
        }

        now
    });

    view! {
        <LookupField
            selected=selected
            choices=Choices::live(choices, on_query)
            disabled=disabled
            id=id
            invalid=invalid
            placeholder=placeholder
        />
    }
}
