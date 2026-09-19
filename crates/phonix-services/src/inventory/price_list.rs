//! Price lists, and what a variant costs in one.
//!
//! Gated on `ITEMS`: a price list is catalogue data, and somebody who may see
//! what the workspace sells may see what it charges for it.
//!
//! The rule that picks between several candidate prices is
//! `app_inventory::price_list::resolve`, not here - it compiles to wasm so the
//! browser prices a line as somebody types, and a second copy in a service
//! would be a second answer.

use app_inventory::price_list::{ItemPrice, PriceList, PriceListInput, resolve};
use app_inventory::quantity::Quantity;
use chrono::NaiveDate;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::price_list as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target};
use crate::caller::{Caller, acting_user};
use crate::error::ServiceResult;

/// Every price list in this workspace.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<PriceList>> {
    caller.require(permissions::ITEMS)?;
    Ok(store::list(pool).await?)
}

/// Every price for one variant in one list.
pub async fn prices_for(
    pool: &PgPool,
    caller: &Caller,
    price_list_id: Uuid,
    variant_id: Uuid,
) -> ServiceResult<Vec<ItemPrice>> {
    caller.require(permissions::ITEMS)?;

    let Some(list) = store::find(pool, price_list_id).await? else {
        return Ok(Vec::new());
    };

    Ok(store::prices_for(pool, price_list_id, variant_id, list.currency).await?)
}

/// What one line would be priced at, or `None` where the list prices nothing
/// that applies.
///
/// `None` is an answer rather than a failure, and the caller decides what it
/// means: a sales order refuses a blank price rather than filling one in, which
/// is the trade `inventory::sales_order` already records.
pub async fn price_for(
    pool: &PgPool,
    caller: &Caller,
    price_list_id: Uuid,
    variant_id: Uuid,
    quantity: Quantity,
    on: NaiveDate,
) -> ServiceResult<Option<ItemPrice>> {
    let prices = prices_for(pool, caller, price_list_id, variant_id).await?;

    Ok(resolve(&prices, quantity, on).cloned())
}

/// The list a customer is quoted from, or `None` for one nobody assigned.
pub async fn for_party(
    pool: &PgPool,
    caller: &Caller,
    party_id: Uuid,
) -> ServiceResult<Option<PriceList>> {
    caller.require(permissions::ITEMS)?;
    Ok(store::for_party(pool, party_id).await?)
}

/// Quote this customer from this list, or from none.
pub async fn assign(
    pool: &PgPool,
    caller: &Caller,
    party_id: Uuid,
    price_list_id: Option<Uuid>,
) -> ServiceResult<()> {
    caller.require(permissions::ITEMS_EDIT)?;
    Ok(store::assign(pool, party_id, price_list_id).await?)
}

/// What a line for this customer would be priced at.
///
/// The whole question a quotation asks, in one call: which list they are on,
/// which price in it applies to this many on this date. `None` where the
/// customer is on no list, or their list does not price the variant, or it
/// prices it only above a quantity this line does not reach.
///
/// The caller decides what `None` means. A sales order refuses a blank price
/// rather than filling one in from `items.sale_price`, which is the trade
/// `inventory::sales_order` records: a stale price on a sales order is revenue
/// given away, and a default nobody chose is the way it goes.
pub async fn quoted_to(
    pool: &PgPool,
    caller: &Caller,
    party_id: Uuid,
    variant_id: Uuid,
    quantity: Quantity,
    on: NaiveDate,
) -> ServiceResult<Option<ItemPrice>> {
    let Some(list) = for_party(pool, caller, party_id).await? else {
        return Ok(None);
    };

    price_for(pool, caller, list.id, variant_id, quantity, on).await
}

/// One list and the prices in it, as the screen edits them.
pub async fn detail(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Option<PriceListInput>> {
    caller.require(permissions::ITEMS)?;

    let Some(list) = store::find(pool, id).await? else {
        return Ok(None);
    };

    let prices = store::prices_in(pool, id, list.currency).await?;

    Ok(Some(PriceListInput::of(&list, &prices)))
}

/// A blank list, opened in the workspace's own currency.
pub async fn blank(pool: &PgPool, caller: &Caller) -> ServiceResult<PriceListInput> {
    caller.require(permissions::ITEMS_EDIT)?;

    let currency = crate::workspace::profile::base_currency(pool).await?;

    Ok(PriceListInput::blank(currency))
}

/// Store a list and its prices, creating or replacing.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: PriceListInput,
) -> ServiceResult<Submission<PriceListInput>> {
    caller.require(permissions::ITEMS_EDIT)?;
    acting_user(caller)?;

    let currency = match Currency::parse(&draft.currency) {
        Ok(currency) => currency,
        Err(_) => {
            return Ok(Submission::rejected(
                "currency",
                msg!("price_lists.error.currency_unknown"),
            ));
        }
    };

    // The browser's check is a courtesy; this one is the control.
    let checked = match draft.check(currency) {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::save(pool, &checked).await {
        Ok(id) => id,
        Err(DbError::CodeExists { code, .. }) => {
            return Ok(Submission::rejected(
                "code",
                msg!("price_lists.error.code_taken", code = code),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    let stored = detail(pool, caller, id)
        .await?
        .unwrap_or_else(|| PriceListInput::blank(currency));

    let before = if draft.id.is_some() {
        "edited"
    } else {
        "created"
    };

    audit::updated(
        pool,
        caller,
        Target::new(kinds::ITEM, id)
            .named(&stored.name)
            .fact("code", &stored.code)
            .fact("prices", checked.prices.len().to_string()),
        &before.to_owned(),
        &"saved".to_owned(),
    )
    .await;

    Ok(Submission::Saved(stored))
}
