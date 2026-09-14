//! Attachments: the paperwork a record came with.
//!
//! # A field is what the system knows; an attachment is what it was told
//!
//! A goods receipt records a delivery-note number. The delivery note itself -
//! the scan with the driver's signature on it - is not a field and never will
//! be, and the moment a dispute starts it is the only thing anybody wants. The
//! same holds for the supplier's invoice behind a bill and the signed contract
//! behind an employment record.
//!
//! # Addressed the way history is
//!
//! An attachment names a record with an [`EntityKind`] and that record's id as
//! text, which is exactly how `entity_events` names one. One vocabulary for
//! "which record is this about", so a screen that can show a record's history
//! can show its attachments without learning a second way to say the same
//! thing - and adding attachments to a new entity costs nothing.
//!
//! The trade is spelled out in migration 0022: `entity_id` cannot be a foreign
//! key, so removing a record has to remove its attachments in the same
//! transaction. That is a rule this crate cannot enforce; see
//! `phonix_services::files::attachment::detach_all`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::upload::{FileId, FileSummary};
use crate::audit::EntityKind;
use crate::i18n::Message;
use crate::msg;

/// The `file_uploads` bucket attachments are stored in.
///
/// Handed back exactly as it arrived - nothing renders an attachment inline -
/// which is why this bucket accepts a PDF where the picture buckets refuse
/// active content.
pub const BUCKET: &str = "attachments";

/// How long a caption may be.
pub const MAX_TITLE_LEN: usize = 200;

/// How many files one record may carry.
///
/// A ceiling rather than a policy: past a couple of dozen this is not the
/// paperwork behind a document any more, it is a folder, and a folder wants a
/// screen of its own rather than a section on a form.
pub const MAX_PER_RECORD: usize = 24;

/// Which record an attachment belongs to.
///
/// The pair is the whole address. Built from an [`EntityKind`] rather than
/// from a loose string so a caller cannot invent a third spelling of
/// `goods_receipt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordRef {
    pub entity_type: String,
    pub entity_id: String,
}

impl RecordRef {
    pub fn new(kind: EntityKind, id: impl std::fmt::Display) -> Self {
        Self {
            entity_type: kind.name.to_owned(),
            entity_id: id.to_string(),
        }
    }

    /// The kind this names, where this build still knows it.
    ///
    /// `None` for a row written by a newer build. The list still renders - the
    /// files are the point, and the kind is only used to title the section.
    pub fn kind(&self) -> Option<EntityKind> {
        crate::audit::ENTITY_KINDS
            .iter()
            .copied()
            .find(|kind| kind.name == self.entity_type)
    }
}

/// One file kept alongside one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub record: RecordRef,
    /// The stored file, with everything a row needs to draw itself: its name,
    /// its size, what it turned out to be, and whether it is ready.
    pub file: FileSummary,
    /// What the filer called it. `None` means the file's own name is the
    /// caption.
    pub title: Option<String>,
    pub attached_by_name: Option<String>,
}

impl Attachment {
    /// What the row is called on screen.
    pub fn label(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.file.original_name.clone())
    }

    /// Where the bytes are. The same address every other file uses.
    pub fn href(&self) -> String {
        format!("/files/{}/content", self.file.id)
    }
}

/// What a screen sends to attach one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub record: RecordRef,
    /// Already uploaded: the bytes went to `/files/upload?bucket=attachments`
    /// and this is what came back. Attaching is only the link.
    pub file_id: FileId,
    pub title: String,
}

impl AttachmentInput {
    /// Trim, and refuse what the table would refuse.
    pub fn check(&self) -> Result<Self, AttachmentError> {
        if self.record.entity_type.trim().is_empty() || self.record.entity_id.trim().is_empty() {
            return Err(AttachmentError::NoRecord);
        }

        let title = self.title.trim();
        if title.chars().count() > MAX_TITLE_LEN {
            return Err(AttachmentError::TitleTooLong);
        }

        Ok(Self {
            record: self.record.clone(),
            file_id: self.file_id,
            title: title.to_owned(),
        })
    }

    /// The caption as it is stored: absent rather than empty.
    pub fn stored_title(&self) -> Option<&str> {
        let title = self.title.trim();
        (!title.is_empty()).then_some(title)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AttachmentError {
    #[error("an attachment has to belong to a record")]
    NoRecord,
    #[error("a caption is at most 200 characters")]
    TitleTooLong,
    #[error("that file is not ready yet")]
    FileNotStored,
    #[error("that file is already attached to this record")]
    AlreadyAttached,
    #[error("this record has as many attachments as it may have")]
    TooMany,
}

impl AttachmentError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NoRecord => "record",
            Self::TitleTooLong => "title",
            Self::FileNotStored | Self::AlreadyAttached | Self::TooMany => "file_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NoRecord => msg!("attachments.error.no_record"),
            Self::TitleTooLong => msg!("attachments.error.title_too_long"),
            Self::FileNotStored => msg!("attachments.error.file_not_stored"),
            Self::AlreadyAttached => msg!("attachments.error.already_attached"),
            Self::TooMany => msg!("attachments.error.too_many"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::kinds;

    fn input(title: &str) -> AttachmentInput {
        AttachmentInput {
            record: RecordRef::new(kinds::RECEIPT, Uuid::nil()),
            file_id: Uuid::nil(),
            title: title.to_owned(),
        }
    }

    #[test]
    fn a_caption_is_trimmed_and_may_be_nothing() {
        let checked = input("  Delivery note  ").check().unwrap();
        assert_eq!(checked.title, "Delivery note");
        assert_eq!(checked.stored_title(), Some("Delivery note"));

        assert_eq!(input("   ").check().unwrap().stored_title(), None);
    }

    #[test]
    fn a_caption_has_a_ceiling() {
        let long = "x".repeat(MAX_TITLE_LEN + 1);
        assert_eq!(input(&long).check(), Err(AttachmentError::TitleTooLong));
    }

    #[test]
    fn a_record_ref_finds_its_kind_back() {
        let record = RecordRef::new(kinds::RECEIPT, Uuid::nil());
        assert_eq!(record.kind(), Some(kinds::RECEIPT));
    }
}
