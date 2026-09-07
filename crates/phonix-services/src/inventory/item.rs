//! Items: creating one, changing one, and the two things that are frozen.
//!
//! # The item code is allocated at create, not at post
//!
//! Unlike an invoice number. An item is not a document and its code is not
//! evidence of anything, so a gap in the series costs nothing - whereas an item
//! that exists without a code is one nothing can refer to. See ADR 0006
//! section 3, which draws the line at "a number an auditor asks about".
//!
//! # Two things are frozen once stock exists
//!
//! * **The stock unit.** Changing it restates every quantity ever recorded:
//!   two hundred kilograms silently becomes two hundred grams.
//! * **The tracking mode.** Turning lots on leaves the existing two hundred
//!   belonging to no lot, and every FEFO pick and every recall afterwards skips
//!   them.
//!
//! Neither is a rule a type can enforce, because both need to know whether
//! anything has been counted. Neither should be discovered during a recall.

use app_inventory::accounts::{AccountOverrides, AccountRef};
use app_inventory::image::{Gallery, ImageError, ImageInput, MAX_IMAGES};
use app_inventory::item::{DeleteOutcome, Item, ItemError, ItemInput, ItemSummary};
use app_inventory::variant::{self, Selection, VariantSummary};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::account_mapping::{self, Owner};
use phonix_db::inventory::{image as images, item as store, variant as variants};
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::AccountRole;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// Every item, with the picture each one shows.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ItemSummary>> {
    caller.require(permissions::ITEMS)?;

    // The column holds a number; the workspace holds what it is denominated in.
    let currency = crate::workspace::profile::current(pool).await?.currency;
    Ok(store::list(pool, currency).await?)
}

/// The one picture each of a set of items shows, for a grid or a till page.
///
/// Separate from [`list`] so a screen that does not draw tiles does not pay for
/// them, and one query for the whole page rather than a gallery per row.
pub async fn tiles(
    pool: &PgPool,
    caller: &Caller,
    item_ids: &[Uuid],
) -> ServiceResult<Vec<(Uuid, Uuid)>> {
    caller.require(permissions::ITEMS)?;
    Ok(images::tiles(pool, item_ids).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Item> {
    caller.require(permissions::ITEMS)?;

    let currency = crate::workspace::profile::current(pool).await?.currency;

    store::find(pool, id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("item", msg!("items.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<ItemInput> {
    Ok(ItemInput::from_item(&detail(pool, caller, id).await?))
}

/// What a scanner produces: the item, and the variant if the code was one of
/// those. One lookup rather than two attempts - the person holding the scanner
/// does not know which table the string is in.
pub async fn by_barcode(
    pool: &PgPool,
    caller: &Caller,
    barcode: &str,
) -> ServiceResult<Option<(Item, Option<Uuid>)>> {
    caller.require(permissions::ITEMS)?;

    let currency = crate::workspace::profile::current(pool).await?.currency;
    Ok(store::by_barcode(pool, barcode.trim(), currency).await?)
}

/// Create an item, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: ItemInput,
) -> ServiceResult<Submission<ItemInput>> {
    match draft.id {
        None => create(pool, caller, draft).await,
        Some(id) => update(pool, caller, id, draft).await,
    }
}

async fn create(
    pool: &PgPool,
    caller: &Caller,
    draft: ItemInput,
) -> ServiceResult<Submission<ItemInput>> {
    caller.require(permissions::ITEMS_CREATE)?;
    acting_user(caller)?;

    // The browser's check is a courtesy; this one is the control.
    let mut checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    if let Err(err) = check_units(pool, &checked).await? {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    // Outside the transaction: every item queues through the sequence's one
    // row, so anything that can happen before the lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if checked.code.is_empty() {
        let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::ITEM);
        match generator
            .next(&mut tx, key, chrono::Utc::now().date_naive())
            .await
        {
            Ok(allocated) => checked.code = allocated.number,
            // The series is missing or off. Reported on the code field,
            // because typing one by hand is the other way out.
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "code",
                    msg!("items.error.code_required"),
                ));
            }
            Err(err) => return Err(err),
        }
    }

    let id = match store::insert(&mut tx, &checked, caller.user_id()).await {
        Ok(id) => id,
        Err(DbError::CodeExists { entity, code }) => {
            // Rolling back returns the number.
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(taken(entity, &code));
        }
        Err(err) => return Err(err.into()),
    };

    tx.commit().await.map_err(DbError::Query)?;

    let stored = ItemInput {
        id: Some(id),
        code: checked.code.clone(),
        ..draft
    };

    audit::created(
        pool,
        caller,
        Target::new(kinds::ITEM, id)
            .named(&checked.name)
            .fact("code", &checked.code)
            .fact("tracking", checked.tracking.as_str()),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

async fn update(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    draft: ItemInput,
) -> ServiceResult<Submission<ItemInput>> {
    caller.require(permissions::ITEMS_EDIT)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    if let Err(err) = check_units(pool, &checked).await? {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    let before = detail(pool, caller, id).await?;

    // The two that are frozen. Both need to know whether anything has been
    // counted, which is why neither is in `ItemInput::check`.
    if has_stock(pool, id).await? {
        if before.stock_unit_id != checked.stock_unit_id {
            return Ok(Submission::rejected(
                "stock_unit_id",
                ItemError::StockUnitLocked.message(),
            ));
        }
        if before.tracking != checked.tracking || before.is_tracked != checked.is_tracked {
            return Ok(Submission::rejected(
                "tracking",
                ItemError::TrackingLocked.message(),
            ));
        }
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    match store::update(&mut tx, id, &checked, caller.user_id()).await {
        Ok(true) => {}
        Ok(false) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected("name", msg!("items.gone")));
        }
        Err(DbError::CodeExists { entity, code }) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(taken(entity, &code));
        }
        Err(err) => return Err(err.into()),
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = ItemInput {
        id: Some(id),
        ..draft
    };

    audit::updated(
        pool,
        caller,
        Target::new(kinds::ITEM, id).named(&checked.name),
        &ItemInput::from_item(&before),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Remove an item nothing has ever been done to.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DeleteOutcome> {
    caller.require(permissions::ITEMS_DELETE)?;
    acting_user(caller)?;

    let item = detail(pool, caller, id).await?;

    if has_stock(pool, id).await? {
        return Ok(DeleteOutcome::HasStock);
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if !removed {
        // Somebody else removed it. "Make it gone" is about the end state.
        return Ok(DeleteOutcome::Deleted);
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::ITEM, id)
            .named(&item.name)
            .fact("code", &item.code),
        &ItemInput::from_item(&item),
    )
    .await;

    Ok(DeleteOutcome::Deleted)
}

// --- Variants ------------------------------------------------------------

/// Every variant of an item, as the variants tab draws them.
pub async fn variants_of(
    pool: &PgPool,
    caller: &Caller,
    item_id: Uuid,
) -> ServiceResult<Vec<VariantSummary>> {
    caller.require(permissions::ITEMS)?;

    let rows = variants::of_item(pool, item_id).await?;
    let gallery = images::gallery(pool, item_id).await?;

    Ok(variants::summarise(&rows, |variant_id| {
        gallery.tile_for_variant(variant_id).map(|image| image.file_id)
    }))
}

/// Which values this item is offered in, for the variants tab to open on.
pub async fn selection(pool: &PgPool, caller: &Caller, item_id: Uuid) -> ServiceResult<Selection> {
    caller.require(permissions::ITEMS)?;
    Ok(variants::selection(pool, item_id).await?)
}

/// What changing the selection would do, without doing it.
///
/// Shown before anything is written. Six colours, five sizes and three
/// materials is ninety variants, and a workspace that meant to add one colour
/// should see that number first.
pub async fn plan_variants(
    pool: &PgPool,
    caller: &Caller,
    item_id: Uuid,
    selection: Selection,
) -> ServiceResult<variant::Plan> {
    caller.require(permissions::ITEMS_EDIT)?;

    let existing = variants::of_item(pool, item_id).await?;

    variant::plan(&selection, &existing)
        .map_err(|err| ServiceError::rejected(err.field(), err.message()))
}

/// Apply a selection: create what is new, retire what is no longer offered,
/// revive what is offered again.
///
/// Retire, never delete. Stock has moved against these and the moves are the
/// audit trail.
pub async fn set_variants(
    pool: &PgPool,
    caller: &Caller,
    item_id: Uuid,
    selection: Selection,
) -> ServiceResult<variant::Plan> {
    caller.require(permissions::ITEMS_EDIT)?;
    acting_user(caller)?;

    let item = detail(pool, caller, item_id).await?;
    let existing = variants::of_item(pool, item_id).await?;

    let plan = variant::plan(&selection, &existing)
        .map_err(|err| ServiceError::rejected(err.field(), err.message()))?;

    // Names, so a new variant's code reads as `ITM-00042-RED-M` rather than as
    // a row of ids.
    let attributes = variants::attributes(pool).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    variants::set_selection(&mut tx, item_id, &selection).await?;

    for combination in &plan.to_create {
        let mut pairs: Vec<(Uuid, Uuid)> = Vec::with_capacity(combination.len());
        let mut names: Vec<String> = Vec::with_capacity(combination.len());

        for value_id in combination {
            let found = attributes.iter().find_map(|attribute| {
                attribute
                    .values
                    .iter()
                    .find(|value| value.id == *value_id)
                    .map(|value| (attribute.id, value.name.clone()))
            });

            // A value that vanished between the plan and the apply. Skipped
            // rather than failing the whole batch: the combination that is
            // left is still a combination somebody asked for.
            if let Some((attribute_id, name)) = found {
                pairs.push((attribute_id, *value_id));
                names.push(name);
            }
        }

        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        let code = variant::variant_code(&item.code, &borrowed);

        variants::create(&mut tx, item_id, &code, &pairs, caller.user_id()).await?;
    }

    for id in &plan.to_retire {
        variants::retire(&mut *tx, *id, false, caller.user_id()).await?;
    }
    for id in &plan.to_revive {
        variants::retire(&mut *tx, *id, true, caller.user_id()).await?;
    }

    tx.commit().await.map_err(DbError::Query)?;

    Ok(plan)
}

/// Change what one variant carries: its own barcode, and what it adds.
pub async fn save_variant(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    barcode: Option<String>,
    price_extra: String,
    cost_extra: String,
) -> ServiceResult<bool> {
    caller.require(permissions::ITEMS_EDIT)?;
    acting_user(caller)?;

    let barcode = barcode.map(|raw| raw.trim().to_owned()).filter(|raw| !raw.is_empty());

    match variants::update(
        pool,
        id,
        barcode.as_deref(),
        amount_or_zero(&price_extra).as_str(),
        amount_or_zero(&cost_extra).as_str(),
        caller.user_id(),
    )
    .await
    {
        Ok(changed) => Ok(changed),
        Err(DbError::CodeExists { code, .. }) => Err(ServiceError::rejected(
            "barcode",
            msg!("items.error.barcode_taken_by", code = code),
        )),
        Err(err) => Err(err.into()),
    }
}

/// Every attribute and its values, for the variants tab's pickers.
pub async fn attributes(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<app_inventory::variant::Attribute>> {
    caller.require(permissions::ITEMS)?;
    Ok(variants::attributes(pool).await?)
}

// --- Pictures ------------------------------------------------------------

/// Every picture an item has, its own and its variants'.
pub async fn gallery(pool: &PgPool, caller: &Caller, item_id: Uuid) -> ServiceResult<Gallery> {
    caller.require(permissions::ITEMS)?;
    Ok(images::gallery(pool, item_id).await?)
}

/// Attach an already-uploaded file to an item, or to one of its variants.
///
/// The upload itself went through `files`, which is what checked that the
/// bucket accepts it and that it is really an image. This only files it.
pub async fn attach_image(
    pool: &PgPool,
    caller: &Caller,
    draft: ImageInput,
) -> ServiceResult<Uuid> {
    caller.require(permissions::ITEMS_EDIT)?;
    acting_user(caller)?;

    let checked = draft
        .check()
        .map_err(|err| ServiceError::rejected(err.field(), err.message()))?;

    let gallery = images::gallery(pool, checked.item_id).await?;
    if gallery.count_for(checked.variant_id) >= MAX_IMAGES {
        return Err(ServiceError::rejected(
            ImageError::TooManyImages.field(),
            ImageError::TooManyImages.message(),
        ));
    }

    match images::attach(pool, &checked, caller.user_id()).await {
        Ok(id) => Ok(id),
        Err(DbError::CodeExists { .. }) => Err(ServiceError::rejected(
            ImageError::AlreadyAttached.field(),
            ImageError::AlreadyAttached.message(),
        )),
        Err(err) => Err(err.into()),
    }
}

/// Take a picture off an item. The stored file itself stays - it may be on
/// another item, and removing it here would be this screen reaching into the
/// file store.
pub async fn detach_image(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::ITEMS_EDIT)?;
    acting_user(caller)?;

    Ok(images::detach(pool, id).await?)
}

/// Move a picture in the gallery.
pub async fn reorder_image(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    position: i32,
) -> ServiceResult<bool> {
    caller.require(permissions::ITEMS_EDIT)?;
    Ok(images::reorder(pool, id, position).await?)
}

// --- Account mapping -----------------------------------------------------

/// What this item and its category override, so a screen can show which of the
/// two an account came from.
pub async fn account_overrides(
    pool: &PgPool,
    caller: &Caller,
    item_id: Uuid,
) -> ServiceResult<(AccountOverrides, AccountOverrides)> {
    caller.require(permissions::ITEMS)?;

    let item = detail(pool, caller, item_id).await?;

    Ok((
        account_mapping::for_owner(pool, Owner::Item, item_id).await?,
        account_mapping::for_owner(pool, Owner::Category, item.category_id).await?,
    ))
}

/// Point one of this item's roles at an account, or stop overriding it.
///
/// `chosen` absent clears the override, so the posting falls back to the
/// category's and then to Books' own default - which is where almost every item
/// should be.
pub async fn set_account(
    pool: &PgPool,
    caller: &Caller,
    owner: Owner,
    owner_id: Uuid,
    role: AccountRole,
    chosen: Option<AccountRef>,
) -> ServiceResult<()> {
    caller.require(match owner {
        Owner::Item => permissions::ITEMS_EDIT,
        Owner::Category => permissions::ITEM_CATEGORIES_MANAGE,
    })?;
    acting_user(caller)?;

    // Only the six an item or a category may speak for. Accounts payable
    // belongs to the supplier, and in-transit is workspace-wide policy.
    if !AccountOverrides::OVERRIDABLE.contains(&role) {
        return Err(ServiceError::rejected(
            "role",
            msg!("items.error.role_not_overridable"),
        ));
    }

    match chosen {
        None => {
            account_mapping::clear(pool, owner, owner_id, role).await?;
        }
        Some(chosen) => {
            account_mapping::set(pool, owner, owner_id, role, &chosen, caller.user_id()).await?;
        }
    }

    Ok(())
}

// --- Shared ---------------------------------------------------------------

/// Whether any stock exists for this item.
///
/// Always false until the stock tables exist. One function so the two frozen
/// fields and the delete cannot disagree about what "has stock" means, and so
/// there is one place to change when the movements arrive.
async fn has_stock(_pool: &PgPool, _item_id: Uuid) -> ServiceResult<bool> {
    Ok(false)
}

/// The purchase unit has to measure the same thing as the stock unit, or a
/// receipt in cases could not be converted into a number of eaches.
async fn check_units(pool: &PgPool, checked: &app_inventory::item::Checked) -> ServiceResult<Result<(), ItemError>> {
    if checked.purchase_unit_id == checked.stock_unit_id {
        return Ok(Ok(()));
    }

    let units = phonix_db::inventory::unit::list(pool).await?;
    let stock = units.iter().find(|unit| unit.id == checked.stock_unit_id);
    let purchase = units.iter().find(|unit| unit.id == checked.purchase_unit_id);

    match (stock, purchase) {
        (Some(stock), Some(purchase)) if stock.class == purchase.class => Ok(Ok(())),
        // A unit that is not there is the picker being stale, which reads to
        // somebody the same way a mismatch does.
        _ => Ok(Err(ItemError::PurchaseUnitMismatch)),
    }
}

/// An empty amount is zero. A variant that adds nothing is the ordinary case.
fn amount_or_zero(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// A duplicate, on whichever field it was actually about. A taken code and a
/// taken barcode are different mistakes.
fn taken(entity: &'static str, code: &str) -> Submission<ItemInput> {
    if entity == "item_barcode" {
        Submission::rejected("barcode", msg!("items.error.barcode_taken_by", code = code))
    } else {
        Submission::rejected("code", msg!("items.error.code_taken", code = code))
    }
}
