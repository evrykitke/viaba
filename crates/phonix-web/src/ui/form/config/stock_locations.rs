//! The location form.
//!
//! # Only three kinds can be typed
//!
//! Internal, a grouping, and transit. The counterpart locations - vendors,
//! customers, inventory loss, production - are seeded once and left alone: a
//! second inventory-loss location would be two places a count difference could
//! go, with nothing to say which. The picker offers what may be created, and
//! the service refuses the rest.
//!
//! # There is no code field
//!
//! The path is derived from the tree - `WH/Stock/Zone A` - so what somebody
//! types is the last segment. A code somebody could type independently of where
//! the row sits is a code that eventually contradicts it.

use app_inventory::location::{LocationInput, LocationKind, LocationSummary};
use app_inventory::warehouse::Warehouse;
use phonix_core::permissions;
use uuid::Uuid;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::save_stock_location;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

/// What "not chosen" is worth, as a select option.
const NOT_SET: &str = "";

/// Where stock can be.
pub fn stock_location_form(
    editing: Option<Uuid>,
    locations: Vec<LocationSummary>,
    warehouses: Vec<Warehouse>,
) -> FormConfig<LocationInput> {
    FormConfig::new("stock-location", |draft: LocationInput| async move {
        save_stock_location(draft).await
    })
    .field(
        Field::text("name", l!("field.name"), |m: &LocationInput| {
            FieldValue::text(&m.name)
        })
        .writing(|m, value| m.name = value.as_input())
        .placeholder("Shelf 1")
        .help(l!("locations.name_help"))
        .require(permissions::STOCK_LOCATIONS_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "kind",
            l!("locations.kind"),
            kind_choices(),
            |m: &LocationInput| FieldValue::choice(m.kind.as_str()),
        )
        // Anything unparseable stays internal, which is what almost every
        // location somebody types by hand actually is.
        .writing(|m, value| {
            m.kind = value
                .as_choice()
                .and_then(LocationKind::parse)
                .filter(|kind| kind.is_user_creatable())
                .unwrap_or(LocationKind::Internal);
        })
        .help(l!("locations.kind_help"))
        .require(permissions::STOCK_LOCATIONS_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "parent_id",
            l!("locations.parent"),
            parent_choices(editing, &locations),
            |m: &LocationInput| {
                FieldValue::choice(
                    m.parent_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| NOT_SET.to_owned()),
                )
            },
        )
        .writing(|m, value| {
            m.parent_id = value
                .as_choice()
                .filter(|raw| !raw.is_empty())
                .and_then(|raw| Uuid::parse_str(raw).ok());
        })
        // A transit location stands outside the warehouse tree, so the field
        // has nothing to offer it.
        .when(|m: &LocationInput| m.kind != LocationKind::Transit)
        .none_label(l!("locations.parent.none"))
        .require(permissions::STOCK_LOCATIONS_MANAGE),
    )
    .field(
        Field::select(
            "warehouse_id",
            l!("locations.warehouse"),
            warehouse_choices(&warehouses),
            |m: &LocationInput| {
                FieldValue::choice(
                    m.warehouse_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| NOT_SET.to_owned()),
                )
            },
        )
        .writing(|m, value| {
            m.warehouse_id = value
                .as_choice()
                .filter(|raw| !raw.is_empty())
                .and_then(|raw| Uuid::parse_str(raw).ok());
        })
        .when(|m: &LocationInput| m.kind != LocationKind::Transit)
        .none_label(l!("locations.warehouse.none"))
        .require(permissions::STOCK_LOCATIONS_MANAGE),
    )
    .field(
        Field::toggle(
            "is_replenished",
            l!("locations.replenished"),
            |m: &LocationInput| FieldValue::Bool(m.is_replenished),
        )
        .writing(|m, value| m.is_replenished = value.as_bool())
        .help(l!("locations.replenished_help"))
        // Only somewhere stock actually sits can be a replenishment target;
        // the service corrects it anyway, and hiding it is kinder than
        // silently unticking it.
        .when(|m: &LocationInput| m.kind.is_on_hand())
        .require(permissions::STOCK_LOCATIONS_MANAGE),
    )
    .field(
        Field::number(
            "count_frequency_days",
            l!("locations.count_frequency"),
            |m: &LocationInput| {
                FieldValue::Number(m.count_frequency_days.map(f64::from))
            },
        )
        .writing(|m, value| {
            m.count_frequency_days = value
                .as_number()
                .filter(|days| *days >= 1.0)
                .map(|days| days as i32);
        })
        .help(l!("locations.count_frequency_help"))
        .when(|m: &LocationInput| m.kind.is_on_hand())
        .require(permissions::STOCK_LOCATIONS_MANAGE),
    )
    .field(
        Field::toggle("is_active", l!("field.in_use"), |m: &LocationInput| {
            FieldValue::Bool(m.is_active)
        })
        .writing(|m, value| m.is_active = value.as_bool())
        .help(l!("locations.active_help"))
        .require(permissions::STOCK_LOCATIONS_MANAGE),
    )
    .action(
        FormAction::submit(l!("common.save"))
            .icon(Icon::Save)
            .then(Then::Say("Location saved."))
            .require(permissions::STOCK_LOCATIONS_MANAGE),
    )
}

/// The three a workspace may create. The counterparts are seeded.
fn kind_choices() -> Vec<Choice> {
    LocationKind::ALL
        .iter()
        .filter(|kind| kind.is_user_creatable())
        .map(|kind| Choice::new(kind.as_str(), crate::i18n::t(&kind.label())))
        .collect()
}

/// Everywhere a location could sit: the groupings and the internal nodes.
///
/// A counterpart location may hold stock and may not hold a *child*, so it is
/// not offered here. Descendants are not filtered out - the service checks
/// those against the tree, and a picker that half-enforced the rule would be
/// trusted.
fn parent_choices(editing: Option<Uuid>, locations: &[LocationSummary]) -> Vec<Choice> {
    let mut choices = Vec::new();

    choices.extend(
        locations
            .iter()
            .filter(|row| Some(row.id) != editing)
            .filter(|row| matches!(row.kind, LocationKind::View | LocationKind::Internal))
            .map(|row| {
                // Non-breaking: a select collapses ordinary whitespace runs.
                let indent = "\u{00a0}\u{00a0}".repeat(row.depth.min(4) as usize);
                Choice::new(row.id.to_string(), format!("{indent}{}", row.name))
            }),
    );

    choices
}

fn warehouse_choices(warehouses: &[Warehouse]) -> Vec<Choice> {
    let mut choices = Vec::new();

    choices.extend(
        warehouses
            .iter()
            .map(|row| Choice::new(row.id.to_string(), format!("{} · {}", row.code, row.name))),
    );

    choices
}
