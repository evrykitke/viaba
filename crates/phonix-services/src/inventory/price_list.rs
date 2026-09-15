//! Price lists, and what a variant costs in one.
//!
//! Gated on `ITEMS`: a price list is catalogue data, and somebody who may see
//! what the workspace sells may see what it charges for it.
//!
//! The rule that picks between several candidate prices is
//! `app_inventory::price_list::resolve`, not here - it compiles to wasm so the
//! browser prices a line as somebody types, and a second copy in a service
//! would be a second answer.

use app_inventory::price_list::{ItemPrice, PriceList, resolve};
use app_inventory::quantity::Quantity;
use chrono::NaiveDate;
use phonix_core::permissions;
use phonix_db::inventory::price_list as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::caller::Caller;
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
