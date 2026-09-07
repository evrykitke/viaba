//! Pictures of what is on the shelf.
//!
//! # Why this is not decoration
//!
//! A point-of-sale screen is a grid of pictures. Somebody serving a queue picks
//! by sight, and a till whose tiles all read `ITM-00042` is slower than the one
//! it replaced. The pictures are the interface.
//!
//! They earn their place before any till exists, too: a goods-in screen where
//! the receiver can see what should be in the box catches the wrong delivery at
//! the door rather than three weeks later during a count.
//!
//! # An image belongs to an item, and optionally to one variant
//!
//! The red shirt has its own photograph. Everything with no photograph of its
//! own falls back to the item's - see [`Gallery::for_variant`], which is the
//! only place that fallback is spelled out, so a till and a catalogue cannot
//! disagree about which picture a variant shows.

use phonix_core::files::FileId;
use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The `core.files` bucket item pictures are stored in.
///
/// Its own rather than `attachments`, because the rules differ in the way that
/// matters: these are rendered **inline**, so the bucket refuses active content
/// and bounds the dimensions. An attachment is handed back as it arrived.
pub const BUCKET: &str = "item-images";

pub const MAX_ALT_TEXT_LEN: usize = 200;

/// How many pictures one item or variant may carry.
///
/// A ceiling because a gallery is scrolled by somebody serving a customer, and
/// past a handful they are looking rather than picking.
pub const MAX_IMAGES: usize = 12;

/// One stored picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub item_id: Uuid,
    /// `None` for the item's own picture, used by every variant with none.
    pub variant_id: Option<Uuid>,
    pub file_id: FileId,
    /// What a screen reader says and what a printed catalogue captions.
    pub alt_text: Option<String>,
    /// Gallery order, low first.
    pub position: i32,
}

impl Image {
    /// Whether this is the item's own rather than one variant's.
    pub const fn is_the_items_own(&self) -> bool {
        self.variant_id.is_none()
    }
}

/// Every picture an item has, its own and its variants'.
///
/// Loaded once for a detail screen or a till page, so choosing a picture is a
/// lookup rather than a query per tile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gallery {
    pub images: Vec<Image>,
}

impl Gallery {
    pub fn new(mut images: Vec<Image>) -> Self {
        // Sorted once, here, so every reader below can take the first.
        images.sort_by_key(|image| (image.variant_id.is_some(), image.position, image.id));
        Self { images }
    }

    /// The item's own pictures, in order.
    pub fn for_item(&self) -> Vec<&Image> {
        self.images
            .iter()
            .filter(|image| image.is_the_items_own())
            .collect()
    }

    /// What one variant shows: its own pictures if it has any, otherwise the
    /// item's.
    ///
    /// The one place the fallback lives. A till and a catalogue calling this
    /// cannot show different pictures for the same variant, which is exactly
    /// what happens when two screens each implement "or the item's".
    pub fn for_variant(&self, variant_id: Uuid) -> Vec<&Image> {
        let own: Vec<&Image> = self
            .images
            .iter()
            .filter(|image| image.variant_id == Some(variant_id))
            .collect();

        if own.is_empty() { self.for_item() } else { own }
    }

    /// The single picture a tile shows for a variant.
    pub fn tile_for_variant(&self, variant_id: Uuid) -> Option<&Image> {
        self.for_variant(variant_id).into_iter().next()
    }

    /// The single picture a tile shows for the item.
    pub fn tile(&self) -> Option<&Image> {
        // Falls through to a variant's picture where the item has none of its
        // own: a grid tile with a photograph of one colour beats an empty
        // square, and an item whose only pictures are on its variants is the
        // normal way somebody uploads them.
        self.for_item()
            .into_iter()
            .next()
            .or_else(|| self.images.first())
    }

    pub const fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// How many pictures the given owner already has, for the "12 at most"
    /// check before an upload.
    pub fn count_for(&self, variant_id: Option<Uuid>) -> usize {
        self.images
            .iter()
            .filter(|image| image.variant_id == variant_id)
            .count()
    }
}

/// A picture somebody is attaching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInput {
    pub item_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub file_id: FileId,
    pub alt_text: String,
    pub position: i32,
}

impl ImageInput {
    pub fn check(&self) -> Result<Self, ImageError> {
        let alt_text = self.alt_text.trim();

        if alt_text.chars().count() > MAX_ALT_TEXT_LEN {
            return Err(ImageError::AltTooLong);
        }

        Ok(Self {
            item_id: self.item_id,
            variant_id: self.variant_id,
            file_id: self.file_id,
            alt_text: alt_text.to_owned(),
            position: self.position.max(0),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("a caption is at most 200 characters")]
    AltTooLong,
    #[error("that picture is already on this item")]
    AlreadyAttached,
    #[error("an item carries at most twelve pictures")]
    TooManyImages,
    #[error("that file is not an image")]
    NotAnImage,
}

impl ImageError {
    pub fn field(self) -> &'static str {
        match self {
            Self::AltTooLong => "alt_text",
            Self::AlreadyAttached | Self::TooManyImages | Self::NotAnImage => "file_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::AltTooLong => msg!("images.error.alt_too_long"),
            Self::AlreadyAttached => msg!("images.error.already_attached"),
            Self::TooManyImages => msg!("images.error.too_many"),
            Self::NotAnImage => msg!("images.error.not_an_image"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn image(n: u128, variant: Option<u128>, position: i32) -> Image {
        Image {
            id: id(n),
            item_id: id(1),
            variant_id: variant.map(id),
            file_id: id(n + 900),
            alt_text: None,
            position,
        }
    }

    #[test]
    fn a_variant_with_no_picture_of_its_own_shows_the_items() {
        // The fallback, in one place, so a till and a catalogue cannot
        // disagree about what the blue shirt looks like.
        let gallery = Gallery::new(vec![image(1, None, 0), image(2, Some(10), 0)]);

        assert_eq!(gallery.tile_for_variant(id(10)).map(|i| i.id), Some(id(2)));
        assert_eq!(gallery.tile_for_variant(id(11)).map(|i| i.id), Some(id(1)));
    }

    #[test]
    fn pictures_come_out_in_the_order_somebody_arranged_them() {
        let gallery = Gallery::new(vec![image(1, None, 2), image(2, None, 0), image(3, None, 1)]);

        let order: Vec<Uuid> = gallery.for_item().iter().map(|image| image.id).collect();
        assert_eq!(order, vec![id(2), id(3), id(1)]);
    }

    #[test]
    fn an_item_whose_only_pictures_are_on_its_variants_still_has_a_tile() {
        // The normal way somebody uploads them, and an empty square in a grid
        // is worse than a photograph of one colour.
        let gallery = Gallery::new(vec![image(2, Some(10), 0)]);

        assert_eq!(gallery.tile().map(|image| image.id), Some(id(2)));
        assert!(gallery.for_item().is_empty());
    }

    #[test]
    fn an_empty_gallery_answers_rather_than_failing() {
        let gallery = Gallery::default();

        assert!(gallery.is_empty());
        assert_eq!(gallery.tile(), None);
        assert_eq!(gallery.tile_for_variant(id(10)), None);
        assert_eq!(gallery.count_for(None), 0);
    }

    #[test]
    fn the_count_is_per_owner_so_a_variant_has_its_own_twelve() {
        let gallery = Gallery::new(vec![
            image(1, None, 0),
            image(2, None, 1),
            image(3, Some(10), 0),
        ]);

        assert_eq!(gallery.count_for(None), 2);
        assert_eq!(gallery.count_for(Some(id(10))), 1);
    }
}
