//! `inventory.categories`: where costing, valuation and picking policy live.
//!
//! Same shape as [`super::location`], including the stored path and the
//! subtree rewrite, and for the same reasons. A few dozen rows.

use app_inventory::category::{Category, CategoryInput, CategorySummary, CostingMethod, RemovalStrategy, Valuation};
use phonix_core::identity::UserId;
use sqlx::{FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const PATH_INDEX: &str = "categories_path";

fn as_conflict(err: sqlx::Error, path: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(PATH_INDEX) => DbError::CodeExists {
            entity: "item_category",
            code: path.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

struct RowOf<T>(T);

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("categories.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_policy(
    row: &sqlx::postgres::PgRow,
) -> Result<(CostingMethod, Valuation, RemovalStrategy), sqlx::Error> {
    let costing: String = row.try_get("costing_method")?;
    let valuation: String = row.try_get("valuation")?;
    let removal: String = row.try_get("removal_strategy")?;

    Ok((
        CostingMethod::parse(&costing).ok_or_else(|| unknown("costing_method", &costing))?,
        Valuation::parse(&valuation).ok_or_else(|| unknown("valuation", &valuation))?,
        RemovalStrategy::parse(&removal).ok_or_else(|| unknown("removal_strategy", &removal))?,
    ))
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Category> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let (costing_method, valuation, removal_strategy) = read_policy(row)?;

        Ok(Self(Category {
            id: row.try_get("id")?,
            code: row.try_get("path")?,
            name: row.try_get("name")?,
            parent_id: row.try_get("parent_id")?,
            costing_method,
            valuation,
            removal_strategy,
            is_active: row.try_get("is_active")?,
        }))
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<CategorySummary> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let (costing_method, valuation, removal_strategy) = read_policy(row)?;

        Ok(Self(CategorySummary {
            id: row.try_get("id")?,
            code: row.try_get("path")?,
            name: row.try_get("name")?,
            parent_id: row.try_get("parent_id")?,
            costing_method,
            valuation,
            removal_strategy,
            is_active: row.try_get("is_active")?,
            depth: 0,
            item_count: row.try_get("item_count")?,
            child_count: row.try_get("child_count")?,
        }))
    }
}

/// Every category, arranged into the tree, with what is filed under each.
pub async fn list<'e, E>(executor: E) -> Result<Vec<CategorySummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<CategorySummary>>(
        "SELECT c.id, c.path, c.name, c.parent_id, c.costing_method, c.valuation,
                c.removal_strategy, c.is_active,
                (SELECT count(*) FROM inventory.categories k WHERE k.parent_id = c.id)
                    AS child_count,
                (SELECT count(*) FROM inventory.items i WHERE i.category_id = c.id)
                    AS item_count
           FROM inventory.categories c
          ORDER BY c.path",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let rows: Vec<CategorySummary> = rows.into_iter().map(|row| row.0).collect();
    Ok(in_tree_order(rows))
}

/// The categories a picker offers.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<Category>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<Category>>(
        "SELECT id, path, name, parent_id, costing_method, valuation,
                removal_strategy, is_active
           FROM inventory.categories
          WHERE is_active
          ORDER BY path",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Category>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Category>>(
        "SELECT id, path, name, parent_id, costing_method, valuation,
                removal_strategy, is_active
           FROM inventory.categories
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

pub async fn insert<'e, E>(
    executor: E,
    path: &str,
    draft: &CategoryInput,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO inventory.categories
             (path, name, parent_id, costing_method, valuation, removal_strategy,
              is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
         RETURNING id",
    )
    .bind(path)
    .bind(&draft.name)
    .bind(draft.parent_id)
    .bind(draft.costing_method.as_str())
    .bind(draft.valuation.as_str())
    .bind(draft.removal_strategy.as_str())
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| as_conflict(err, path))
}

pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    path: &str,
    draft: &CategoryInput,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.categories
            SET path             = $2,
                name             = $3,
                parent_id        = $4,
                costing_method   = $5,
                valuation        = $6,
                removal_strategy = $7,
                is_active        = $8,
                updated_at       = now(),
                updated_by       = $9
          WHERE id = $1",
    )
    .bind(id)
    .bind(path)
    .bind(&draft.name)
    .bind(draft.parent_id)
    .bind(draft.costing_method.as_str())
    .bind(draft.valuation.as_str())
    .bind(draft.removal_strategy.as_str())
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_conflict(err, path))?;

    Ok(result.rows_affected() > 0)
}

/// Rewrite the paths beneath a renamed or moved category. See
/// [`super::location::rename_subtree`].
pub async fn rename_subtree<'e, E>(
    executor: E,
    id: Uuid,
    old_path: &str,
    new_path: &str,
) -> Result<u64, DbError>
where
    E: PgExecutor<'e>,
{
    if old_path == new_path {
        return Ok(0);
    }

    let result = sqlx::query(
        "UPDATE inventory.categories
            SET path = $3 || substring(path FROM char_length($2) + 1),
                updated_at = now()
          WHERE id <> $1
            AND path LIKE $2 || '/%'",
    )
    .bind(id)
    .bind(old_path)
    .bind(new_path)
    .execute(executor)
    .await
    .map_err(|err| as_conflict(err, new_path))?;

    Ok(result.rows_affected())
}

pub async fn descendants<'e, E>(executor: E, id: Uuid) -> Result<Vec<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "WITH RECURSIVE below AS (
             SELECT id FROM inventory.categories WHERE parent_id = $1
             UNION
             SELECT c.id FROM inventory.categories c JOIN below b ON c.parent_id = b.id
         )
         SELECT id FROM below",
    )
    .bind(id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)
}

/// How many items are filed here, asked before a delete.
pub async fn item_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM inventory.items WHERE category_id = $1")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM inventory.categories WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// Arrange a flat list into the tree and say how deep each row is.
///
/// The same walk `app_inventory::location::in_tree_order` does. Not shared
/// with it: sharing would mean a trait over "has a parent and a depth", and a
/// trait extracted for two callers is a `dyn` in front of a loop.
fn in_tree_order(rows: Vec<CategorySummary>) -> Vec<CategorySummary> {
    let present: std::collections::HashSet<Uuid> = rows.iter().map(|row| row.id).collect();

    let mut children: std::collections::HashMap<Option<Uuid>, Vec<CategorySummary>> =
        std::collections::HashMap::new();
    for row in rows {
        let parent = row.parent_id.filter(|id| present.contains(id));
        children.entry(parent).or_default().push(row);
    }

    let mut ordered = Vec::new();
    let mut stack: Vec<(CategorySummary, u16)> = Vec::new();

    if let Some(mut roots) = children.remove(&None) {
        roots.reverse();
        stack.extend(roots.into_iter().map(|row| (row, 0)));
    }

    while let Some((mut row, depth)) = stack.pop() {
        row.depth = depth;
        let id = row.id;
        ordered.push(row);

        if let Some(mut batch) = children.remove(&Some(id)) {
            batch.reverse();
            let depth = depth.saturating_add(1);
            stack.extend(batch.into_iter().map(|row| (row, depth)));
        }
    }

    for (_, batch) in children {
        ordered.extend(batch);
    }

    ordered
}
