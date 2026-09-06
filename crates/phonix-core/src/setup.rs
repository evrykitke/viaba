//! What an app cannot work without, and whether this workspace has it.
//!
//! Defaults (`config/defaults/<app_id>.toml`) mean the list is usually already
//! green on the first morning. This is for the things nobody can guess - an
//! opening balance, which account receivables post to - so that they are
//! discovered on the app's home page rather than by an accountant in March.
//!
//! An app declares its items; `phonix_services::workspace::setup` answers them
//! against a tenant database. Declaration lives here because it compiles to
//! wasm and the browser draws the list; the predicates cannot, for the same
//! reason the migration registry is not in [`crate::apps`].
//!
//! See `docs/adr/0006-apps-ports-and-defaults.md` section 4.

use serde::{Deserialize, Serialize};

use crate::i18n::Message;

/// One thing an app needs before it is useful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupItem {
    /// Stable across releases, and unique within the app.
    pub key: &'static str,

    /// Message key for the line in the list.
    pub label: &'static str,

    /// The screen that satisfies it.
    pub href: &'static str,

    /// Whether the app refuses to post while this is unsatisfied.
    ///
    /// Most items are advisory. A blocking one is a thing whose absence would
    /// otherwise surface as a foreign-key violation from four layers down, and
    /// the point of the flag is that the refusal names what is missing instead.
    pub blocking: bool,

    /// Message key for the sentence said when it is undone: the note beside an
    /// unsatisfied line, and the refusal a blocking one produces.
    pub missing: &'static str,
}

impl SetupItem {
    /// Worth doing. The app works without it.
    pub const fn advisory(
        key: &'static str,
        label: &'static str,
        href: &'static str,
        missing: &'static str,
    ) -> Self {
        Self {
            key,
            label,
            href,
            blocking: false,
            missing,
        }
    }

    /// Required. The app refuses to post until it is done.
    pub const fn blocking(
        key: &'static str,
        label: &'static str,
        href: &'static str,
        missing: &'static str,
    ) -> Self {
        Self {
            key,
            label,
            href,
            blocking: true,
            missing,
        }
    }
}

/// One item, answered for one workspace.
///
/// Carries its own words rather than a key the browser looks up, so a home page
/// renders a checklist without compiling in every app's declarations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupStatus {
    pub key: String,
    pub label: Message,
    pub href: String,
    pub blocking: bool,
    pub satisfied: bool,

    /// What was found, or what is missing: "312 accounts", "not entered".
    pub note: Option<Message>,
}

impl SetupStatus {
    /// An unsatisfied item notes what is missing without being asked: that
    /// sentence is the whole reason the item is on the list.
    pub fn of(item: &SetupItem, satisfied: bool) -> Self {
        Self {
            key: item.key.to_owned(),
            label: Message::new(item.label),
            href: item.href.to_owned(),
            blocking: item.blocking,
            satisfied,
            note: (!satisfied).then(|| Message::new(item.missing)),
        }
    }

    #[must_use]
    pub fn noted(mut self, note: Message) -> Self {
        self.note = Some(note);
        self
    }
}

/// How far along a checklist is: how many are done, out of how many.
pub fn progress(statuses: &[SetupStatus]) -> (usize, usize) {
    (
        statuses.iter().filter(|status| status.satisfied).count(),
        statuses.len(),
    )
}

/// The blocking items this workspace has not done.
///
/// Empty is the answer that lets a document be posted.
pub fn gaps(statuses: &[SetupStatus]) -> Vec<&SetupStatus> {
    statuses
        .iter()
        .filter(|status| status.blocking && !status.satisfied)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: &[SetupItem] = &[
        SetupItem::blocking("chart", "a.label", "/here", "a.missing"),
        SetupItem::advisory("opening", "b.label", "/here", "b.missing"),
    ];

    fn answered(satisfied: [bool; 2]) -> Vec<SetupStatus> {
        ITEMS
            .iter()
            .zip(satisfied)
            .map(|(item, done)| SetupStatus::of(item, done))
            .collect()
    }

    #[test]
    fn an_item_answers_with_its_own_declaration() {
        let status = SetupStatus::of(&ITEMS[0], false);

        assert_eq!(status.key, "chart");
        assert_eq!(status.label, Message::new("a.label"));
        assert_eq!(status.href, "/here");
        assert!(status.blocking);
        assert_eq!(status.note, Some(Message::new("a.missing")));

        // Satisfied, it has nothing to say until a caller notes what it found.
        assert!(SetupStatus::of(&ITEMS[0], true).note.is_none());
    }

    #[test]
    fn progress_counts_the_satisfied_ones() {
        assert_eq!(progress(&answered([false, false])), (0, 2));
        assert_eq!(progress(&answered([true, false])), (1, 2));
        assert_eq!(progress(&answered([true, true])), (2, 2));
        assert_eq!(progress(&[]), (0, 0));
    }

    #[test]
    fn only_an_unsatisfied_blocking_item_is_a_gap() {
        // An advisory item left undone is not a reason to refuse a posting -
        // which is the whole difference between the two kinds.
        assert_eq!(gaps(&answered([true, false])).len(), 0);
        assert_eq!(gaps(&answered([false, true])).len(), 1);
        assert_eq!(gaps(&answered([false, false])).len(), 1);
    }

    #[test]
    fn a_note_is_what_was_found() {
        let status = SetupStatus::of(&ITEMS[0], true)
            .noted(Message::new("a.found").count(312));

        assert_eq!(status.note.and_then(|note| note.count), Some(312));
    }
}
