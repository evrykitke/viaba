//! `inventory.price_lists` and the prices in them.
//!
//! The resolution rule is not here. Which of several prices wins is
//! `app_inventory::price_list::resolve`, in code the browser has too, so a
//! quotation can be priced as somebody types without asking the server. This
//! module's job is to hand over the candidates for one variant in one list.

use app_inventory::price_list::{ItemPrice, PriceList};
use app_inventory::quantity::Quantity;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const CODE_INDEX: &str = "price_lists_code_key";

/// Every list, in code order.
pub async fn list<'e, E>(executor: E) -> Result<Vec<PriceList>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, code, name, currency_code, is_active
           FROM inventory.price_lists
          ORDER BY code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter().map(read_list).collect()
}

/// One list by id, or `None` for one that is not there.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<PriceList>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, code, name, currency_code, is_active
           FROM inventory.price_lists
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.as_ref().map(read_list).transpose()
}

fn read_list(row: &sqlx::postgres::PgRow) -> Result<PriceList, DbError> {
    let code: String = row.try_get("currency_code").map_err(DbError::Query)?;

    let currency = Currency::parse(&code).map_err(|_| {
        DbError::Query(sqlx::Error::Decode(
            format!("price_lists.currency_code holds '{code}', which this build does not know")
                .into(),
        ))
    })?;

    Ok(PriceList {
        id: row.try_get("id").map_err(DbError::Query)?,
        code: row.try_get("code").map_err(DbError::Query)?,
        name: row.try_get("name").map_err(DbError::Query)?,
        currency,
        is_active: row.try_get("is_active").map_err(DbError::Query)?,
    })
}

/// Every price for one variant in one list, for `resolve` to choose between.
///
/// Bounded by how many ways one workspace prices one thing - a handful of
/// breaks and a promotion or two - rather than by the catalogue or the year, so
/// it is read whole on purpose. Ordered so the answer is stable when two rows
/// are equally specific.
pub async fn prices_for<'e, E>(
    executor: E,
    price_list_id: Uuid,
    variant_id: Uuid,
    currency: Currency,
) -> Result<Vec<ItemPrice>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, price_list_id, variant_id,
                min_quantity::text AS min_quantity,
                valid_from, valid_to,
                unit_price::text AS unit_price
           FROM inventory.item_prices
          WHERE price_list_id = $1 AND variant_id = $2
          ORDER BY min_quantity DESC, valid_from DESC NULLS LAST, id",
    )
    .bind(price_list_id)
    .bind(variant_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            let min_quantity: String = row.try_get("min_quantity").map_err(DbError::Query)?;
            let unit_price: String = row.try_get("unit_price").map_err(DbError::Query)?;

            Ok(ItemPrice {
                id: row.try_get("id").map_err(DbError::Query)?,
                price_list_id: row.try_get("price_list_id").map_err(DbError::Query)?,
                variant_id: row.try_get("variant_id").map_err(DbError::Query)?,
                min_quantity: read_quantity(&min_quantity)?,
                valid_from: row.try_get("valid_from").map_err(DbError::Query)?,
                valid_to: row.try_get("valid_to").map_err(DbError::Query)?,
                unit_price: read_money(&unit_price, currency)?,
            })
        })
        .collect()
}

fn read_money(raw: &str, currency: Currency) -> Result<Money, DbError> {
    Money::parse(currency, raw).map_err(|err| {
        DbError::Query(sqlx::Error::Decode(
            format!("item_prices.unit_price holds '{raw}': {err}").into(),
        ))
    })
}

fn read_quantity(raw: &str) -> Result<Quantity, DbError> {
    Quantity::parse(raw).map_err(|err| {
        DbError::Query(sqlx::Error::Decode(
            format!("item_prices.min_quantity holds '{raw}': {err}").into(),
        ))
    })
}

/// Turn the unique-index violation into something a form can render.
pub fn as_code_conflict(err: sqlx::Error, code: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(CODE_INDEX) => DbError::CodeExists {
            entity: "price_list",
            code: code.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

/// The list a customer is quoted from, or `None` for one nobody assigned.
///
/// `None` is the ordinary case rather than a gap: a workspace with one set of
/// prices assigns nobody, and every quotation falls back the same way.
pub async fn for_party<'e, E>(executor: E, party_id: Uuid) -> Result<Option<PriceList>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT p.id, p.code, p.name, p.currency_code, p.is_active
           FROM inventory.party_price_lists a
           JOIN inventory.price_lists p ON p.id = a.price_list_id
          WHERE a.party_id = $1 AND p.is_active",
    )
    .bind(party_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.as_ref().map(read_list).transpose()
}

/// Quote this customer from this list from now on, or stop quoting them from
/// one at all.
pub async fn assign<'e, E>(
    executor: E,
    party_id: Uuid,
    price_list_id: Option<Uuid>,
) -> Result<(), DbError>
where
    E: PgExecutor<'e>,
{
    match price_list_id {
        Some(price_list_id) => {
            sqlx::query(
                "INSERT INTO inventory.party_price_lists (party_id, price_list_id)
                 VALUES ($1, $2)
                 ON CONFLICT (party_id)
                 DO UPDATE SET price_list_id = EXCLUDED.price_list_id, updated_at = now()",
            )
            .bind(party_id)
            .bind(price_list_id)
            .execute(executor)
            .await
            .map_err(DbError::Query)?;
        }
        None => {
            sqlx::query("DELETE FROM inventory.party_price_lists WHERE party_id = $1")
                .bind(party_id)
                .execute(executor)
                .await
                .map_err(DbError::Query)?;
        }
    }

    Ok(())
}
