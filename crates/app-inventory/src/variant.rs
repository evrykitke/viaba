//! Variants: the red medium shirt, as opposed to the shirt.
//!
//! # Stock belongs to a variant, and this is not negotiable
//!
//! A blue medium and a red large are different things on a shelf. They are
//! counted separately, picked separately, barcoded separately and often cost
//! different amounts. Everything that records a quantity therefore names a
//! [`Variant`], never an [`Item`](crate::item::Item).
//!
//! # Every item has at least one variant
//!
//! An item that varies by nothing has exactly one, flagged
//! [`Variant::is_default`], whose combination is empty. That is what lets the
//! rest of the system speak only of variants: there is no second code path for
//! "an item without variants", because there is no such thing.
//!
//! # Generating the combinations is a deliberate act
//!
//! Six colours, five sizes and three materials is ninety variants, and a
//! workspace that meant to add one colour should be told that before ninety
//! rows appear. [`plan`] works out what the cross product *would* be and what
//! it would do to what already exists; the service applies it once somebody has
//! seen the number.

use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_ATTRIBUTE_NAME_LEN: usize = 60;
pub const MAX_VALUE_NAME_LEN: usize = 60;

/// How many variants one item may have.
///
/// Four attributes of five values each is 625, and past roughly this point the
/// workspace wants separate items rather than one item nobody can scroll. A
/// ceiling somebody hits and reads is better than a save that takes a minute.
pub const MAX_VARIANTS_PER_ITEM: usize = 1000;

/// How a picker draws an attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    /// A dropdown. Right once there are more than a handful.
    Select,
    /// Buttons in a row. Right for two to five.
    Radio,
    /// Swatches. The only one that reads at a glance on a till.
    Colour,
}

impl Display {
    pub const ALL: &'static [Self] = &[Self::Select, Self::Radio, Self::Colour];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Radio => "radio",
            Self::Colour => "colour",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|d| d.as_str() == raw)
    }

    /// Whether values of this attribute carry a colour.
    pub const fn uses_swatches(self) -> bool {
        matches!(self, Self::Colour)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Select => msg!("variants.display.select"),
            Self::Radio => msg!("variants.display.radio"),
            Self::Colour => msg!("variants.display.colour"),
        }
    }
}

/// Something items vary by: Colour, Size, Material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub id: Uuid,
    pub name: String,
    pub display: Display,
    pub position: i32,
    pub is_active: bool,
    pub values: Vec<AttributeValue>,
}

/// One value of an attribute: Red, Medium, Oak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeValue {
    pub id: Uuid,
    pub attribute_id: Uuid,
    pub name: String,
    /// `#c0392b`, for a swatch. `None` for everything that is not a colour.
    pub swatch: Option<String>,
    pub position: i32,
    pub is_active: bool,
}

/// One combination an item is actually sold in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub id: Uuid,
    pub item_id: Uuid,
    /// The item's code with the combination appended: `ITM-00042-RED-M`.
    pub code: String,
    /// Its own UPC, typed. The commonest single reason to need variants.
    pub barcode: Option<String>,
    /// What this combination adds to the item's price. Usually zero.
    pub price_extra: String,
    pub cost_extra: String,
    /// The one an item that varies by nothing has, and the one a document
    /// falls back to.
    pub is_default: bool,
    pub is_active: bool,
    /// The combination, in attribute order. Empty for the default variant.
    pub values: Vec<VariantValue>,
}

impl Variant {
    /// `Red / Medium`, or the item's own name for the default variant.
    ///
    /// The one place a combination becomes words, so a grid, a picking list and
    /// a till tile cannot spell it three ways.
    pub fn combination_label(&self) -> Option<String> {
        if self.values.is_empty() {
            return None;
        }

        Some(
            self.values
                .iter()
                .map(|value| value.value_name.as_str())
                .collect::<Vec<_>>()
                .join(" / "),
        )
    }

    /// The ids of the combination, sorted, for comparing two variants.
    pub fn signature(&self) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> = self.values.iter().map(|value| value.value_id).collect();
        ids.sort_unstable();
        ids
    }
}

/// One attribute's answer for one variant, with the names snapshotted so a grid
/// draws without a join per row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantValue {
    pub attribute_id: Uuid,
    pub attribute_name: String,
    pub value_id: Uuid,
    pub value_name: String,
    pub swatch: Option<String>,
}

/// One row of a variants tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantSummary {
    pub id: Uuid,
    pub code: String,
    pub barcode: Option<String>,
    pub combination: Option<String>,
    pub is_default: bool,
    pub is_active: bool,
    /// On hand across every internal location, in the item's stock unit.
    pub on_hand: Option<crate::quantity::Quantity>,
    /// The picture a till shows. `None` falls back to the item's.
    pub image_file_id: Option<Uuid>,
}

/// One variant a document line can name, with the item it belongs to.
///
/// Flat rather than nested under its item: a line names a variant, and a picker
/// that made somebody choose an item and then a variant would be two steps for
/// the ninety per cent of items that have exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantChoice {
    pub id: Uuid,
    pub code: String,
    pub item_name: String,
    pub combination: Option<String>,
    /// What the item is bought in, which is what a new order line defaults to.
    pub purchase_unit_id: Uuid,
    pub purchase_unit_code: String,
}

impl VariantChoice {
    pub fn label(&self) -> String {
        match &self.combination {
            Some(combination) => format!("{} - {combination}", self.item_name),
            None => self.item_name.clone(),
        }
    }
}

/// Which values of which attributes an item is offered in.
///
/// Values rather than attributes: a shirt that comes in red and blue but not
/// green is three facts, and "varies by colour" would offer every colour the
/// workspace has ever used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Each attribute, and the values chosen for it. Order is display order.
    pub lines: Vec<SelectionLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionLine {
    pub attribute_id: Uuid,
    pub attribute_name: String,
    pub value_ids: Vec<Uuid>,
}

impl Selection {
    /// How many variants this selection describes.
    ///
    /// The product of the value counts, and 1 for a selection with no lines -
    /// which is the default variant, not zero variants.
    pub fn combination_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| !line.value_ids.is_empty())
            .map(|line| line.value_ids.len())
            .product::<usize>()
            .max(1)
    }

    /// Every combination, as a sorted list of value ids each.
    ///
    /// An empty selection yields one empty combination: the default variant.
    /// Refused past [`MAX_VARIANTS_PER_ITEM`] rather than built, because the
    /// thousand-and-first row is the one nobody wanted.
    pub fn combinations(&self) -> Result<Vec<Vec<Uuid>>, VariantError> {
        if self.combination_count() > MAX_VARIANTS_PER_ITEM {
            return Err(VariantError::TooManyCombinations {
                wanted: self.combination_count(),
            });
        }

        let mut combinations: Vec<Vec<Uuid>> = vec![Vec::new()];

        for line in &self.lines {
            if line.value_ids.is_empty() {
                continue;
            }

            let mut next = Vec::with_capacity(combinations.len() * line.value_ids.len());
            for existing in &combinations {
                for value_id in &line.value_ids {
                    let mut combination = existing.clone();
                    combination.push(*value_id);
                    next.push(combination);
                }
            }
            combinations = next;
        }

        for combination in &mut combinations {
            combination.sort_unstable();
        }

        Ok(combinations)
    }
}

/// What regenerating an item's variants would do.
///
/// Returned and shown before anything is written. The counts are the point: a
/// workspace deleting an attribute value should see "this retires 30 variants"
/// rather than discover it afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Combinations with no variant yet.
    pub to_create: Vec<Vec<Uuid>>,
    /// Variants whose combination is no longer offered.
    ///
    /// **Retired, never deleted.** Stock has moved against them and the moves
    /// are the audit trail; a variant nobody can sell any more is one that is
    /// switched off, and its history stays legible.
    pub to_retire: Vec<Uuid>,
    /// Retired variants whose combination is offered again.
    pub to_revive: Vec<Uuid>,
    /// Left exactly as they are.
    pub unchanged: usize,
}

impl Plan {
    pub const fn changes_nothing(&self) -> bool {
        self.to_create.is_empty() && self.to_retire.is_empty() && self.to_revive.is_empty()
    }
}

/// Work out what the item's variants should become.
///
/// Pure, so a screen can show the plan and the service can apply the same one.
pub fn plan(selection: &Selection, existing: &[Variant]) -> Result<Plan, VariantError> {
    let wanted = selection.combinations()?;

    let mut to_create = Vec::new();
    let mut to_revive = Vec::new();
    let mut unchanged = 0;
    let mut matched: Vec<Uuid> = Vec::new();

    for combination in wanted {
        match existing
            .iter()
            .find(|variant| variant.signature() == combination)
        {
            None => to_create.push(combination),
            Some(variant) => {
                matched.push(variant.id);
                if variant.is_active {
                    unchanged += 1;
                } else {
                    to_revive.push(variant.id);
                }
            }
        }
    }

    let to_retire = existing
        .iter()
        .filter(|variant| variant.is_active && !matched.contains(&variant.id))
        .map(|variant| variant.id)
        .collect();

    Ok(Plan {
        to_create,
        to_retire,
        to_revive,
        unchanged,
    })
}

/// Build the code for one combination: the item's code, then each value's name
/// reduced to something a label printer and a URL both survive.
pub fn variant_code(item_code: &str, values: &[&str]) -> String {
    if values.is_empty() {
        return item_code.to_owned();
    }

    let mut code = item_code.to_owned();
    for value in values {
        let slug: String = value
            .chars()
            .filter_map(|c| {
                if c.is_ascii_alphanumeric() {
                    Some(c.to_ascii_uppercase())
                } else if c.is_whitespace() || c == '-' || c == '_' {
                    Some('-')
                } else {
                    None
                }
            })
            .collect();

        // Collapse the runs a filter leaves behind: "Dark  Blue" is one hyphen.
        let mut cleaned = String::with_capacity(slug.len());
        let mut last_was_hyphen = false;
        for c in slug.chars() {
            if c == '-' {
                if !last_was_hyphen && !cleaned.is_empty() {
                    cleaned.push('-');
                }
                last_was_hyphen = true;
            } else {
                cleaned.push(c);
                last_was_hyphen = false;
            }
        }

        let trimmed = cleaned.trim_end_matches('-');
        if !trimmed.is_empty() {
            code.push('-');
            code.push_str(trimmed);
        }
    }

    code
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VariantError {
    #[error("an attribute needs a name")]
    AttributeNameRequired,
    #[error("an attribute name is at most sixty characters")]
    AttributeNameTooLong,
    #[error("a value needs a name")]
    ValueNameRequired,
    #[error("a value name is at most sixty characters")]
    ValueNameTooLong,
    #[error("a swatch is a six-digit hex colour, like #c0392b")]
    SwatchShape,
    #[error("that would make {wanted} variants, which is more than one item should carry")]
    TooManyCombinations { wanted: usize },
    #[error("the default variant of an item cannot be removed")]
    DefaultVariantRemoved,
    #[error("stock has moved against that variant, so it is retired rather than deleted")]
    VariantHasMovements,
}

impl VariantError {
    pub fn field(self) -> &'static str {
        match self {
            Self::AttributeNameRequired | Self::AttributeNameTooLong => "name",
            Self::ValueNameRequired | Self::ValueNameTooLong => "value_name",
            Self::SwatchShape => "swatch",
            Self::TooManyCombinations { .. } => "values",
            Self::DefaultVariantRemoved | Self::VariantHasMovements => "variants",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::AttributeNameRequired => msg!("variants.error.attribute_name_required"),
            Self::AttributeNameTooLong => msg!("variants.error.attribute_name_too_long"),
            Self::ValueNameRequired => msg!("variants.error.value_name_required"),
            Self::ValueNameTooLong => msg!("variants.error.value_name_too_long"),
            Self::SwatchShape => msg!("variants.error.swatch_shape"),
            Self::TooManyCombinations { wanted } => {
                msg!("variants.error.too_many", count = wanted)
            }
            Self::DefaultVariantRemoved => msg!("variants.error.default_removed"),
            Self::VariantHasMovements => msg!("variants.error.has_movements"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn selection(lines: &[(u128, &[u128])]) -> Selection {
        Selection {
            lines: lines
                .iter()
                .map(|(attribute, values)| SelectionLine {
                    attribute_id: id(*attribute),
                    attribute_name: format!("Attribute {attribute}"),
                    value_ids: values.iter().map(|value| id(*value)).collect(),
                })
                .collect(),
        }
    }

    fn variant(n: u128, values: &[u128], active: bool) -> Variant {
        Variant {
            id: id(n),
            item_id: id(1),
            code: format!("ITM-{n:05}"),
            barcode: None,
            price_extra: "0".to_owned(),
            cost_extra: "0".to_owned(),
            is_default: values.is_empty(),
            is_active: active,
            values: values
                .iter()
                .map(|value| VariantValue {
                    attribute_id: id(100),
                    attribute_name: "Colour".to_owned(),
                    value_id: id(*value),
                    value_name: format!("Value {value}"),
                    swatch: None,
                })
                .collect(),
        }
    }

    #[test]
    fn an_item_that_varies_by_nothing_still_has_one_variant() {
        // The invariant everything downstream relies on: there is no such thing
        // as an item without a variant, so nothing needs a second code path.
        let plan = plan(&Selection::default(), &[]).unwrap();

        assert_eq!(plan.to_create, vec![Vec::<Uuid>::new()]);
        assert_eq!(Selection::default().combination_count(), 1);
    }

    #[test]
    fn two_attributes_make_the_cross_product() {
        let two_by_three = selection(&[(10, &[1, 2]), (20, &[3, 4, 5])]);

        assert_eq!(two_by_three.combination_count(), 6);
        assert_eq!(two_by_three.combinations().unwrap().len(), 6);

        // Every combination is distinct, which is the whole point of a cross
        // product and the thing an off-by-one in it would break silently.
        let mut all = two_by_three.combinations().unwrap();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 6);
    }

    #[test]
    fn a_ludicrous_selection_is_refused_before_it_is_built() {
        let huge = Selection {
            lines: (0..4)
                .map(|attribute| SelectionLine {
                    attribute_id: id(attribute),
                    attribute_name: "A".to_owned(),
                    value_ids: (0..10).map(|value| id(attribute * 100 + value + 1)).collect(),
                })
                .collect(),
        };

        // 10 000, and the message says so rather than the save simply taking a
        // minute and producing a screen nobody can scroll.
        assert_eq!(
            huge.combinations(),
            Err(VariantError::TooManyCombinations { wanted: 10_000 })
        );
    }

    #[test]
    fn dropping_a_value_retires_its_variants_rather_than_deleting_them() {
        // Stock has moved against them and the moves are the audit trail.
        let existing = vec![variant(2, &[1], true), variant(3, &[2], true)];
        let now_only_red = selection(&[(10, &[1])]);

        let plan = plan(&now_only_red, &existing).unwrap();

        assert_eq!(plan.unchanged, 1);
        assert_eq!(plan.to_retire, vec![id(3)]);
        assert!(plan.to_create.is_empty());
    }

    #[test]
    fn putting_a_value_back_revives_the_variant_it_had() {
        // Rather than making a second one, which would split the history of
        // one shelf across two rows.
        let existing = vec![variant(2, &[1], true), variant(3, &[2], false)];
        let both_again = selection(&[(10, &[1, 2])]);

        let plan = plan(&both_again, &existing).unwrap();

        assert_eq!(plan.to_revive, vec![id(3)]);
        assert!(plan.to_create.is_empty());
        assert!(plan.to_retire.is_empty());
        assert!(!plan.changes_nothing());
    }

    #[test]
    fn a_plan_that_changes_nothing_says_so() {
        let existing = vec![variant(2, &[1], true)];
        assert!(plan(&selection(&[(10, &[1])]), &existing)
            .unwrap()
            .changes_nothing());
    }

    #[test]
    fn a_variant_code_survives_a_label_printer() {
        assert_eq!(variant_code("ITM-00042", &[]), "ITM-00042");
        assert_eq!(
            variant_code("ITM-00042", &["Red", "Medium"]),
            "ITM-00042-RED-MEDIUM"
        );
        // Punctuation dropped, runs collapsed.
        assert_eq!(
            variant_code("ITM-1", &["Dark  Blue", "12\" / 30cm"]),
            "ITM-1-DARK-BLUE-12-30CM"
        );
    }

    #[test]
    fn a_combination_reads_as_words_and_the_default_reads_as_nothing() {
        assert_eq!(variant(2, &[1], true).combination_label().as_deref(), Some("Value 1"));
        assert_eq!(variant(1, &[], true).combination_label(), None);
    }
}
