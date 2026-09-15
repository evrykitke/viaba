//! `inventory.locations`: where stock is, including the places that are not
//! places.
//!
//! [`list`] reads them all for the forms, and arranges them with
//! `app_inventory::location::in_tree_order` - a walk of the parent chain, which
//! also places a row whose parent a filter removed. [`page`] cannot walk what it
//! has not fetched, so it takes tree order and depth from the stored path
//! instead.
//!
//! # The path is stored, and rewriting it is this module's job
//!
//! `WH/Stock/Zone A/Shelf 1` is what every picker sees, and rebuilding it per
//! row in a grid is a query per row. It is derived from the tree, so renaming
//! or moving a node has to rewrite the whole subtree - [`rename_subtree`]. A
//! stored path nobody maintains is worse than no stored path.

use app_inventory::location::{Location, LocationInput, LocationKind, LocationSummary};
use phonix_core::identity::UserId;
use phonix_core::query::{Page, PageRequest};
use sqlx::{AssertSqlSafe, FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::listing::{self, Sortable};

const PATH_INDEX: &str = "locations_path";

fn as_conflict(err: sqlx::Error, path: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(PATH_INDEX) => DbError::CodeExists {
            entity: "location",
            code: path.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

struct RowOf<T>(T);

fn read_kind(raw: &str) -> Result<LocationKind, sqlx::Error> {
    LocationKind::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("locations.kind holds '{raw}', which this build does not know").into(),
        )
    })
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Location> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let kind: String = row.try_get("kind")?;

        Ok(Self(Location {
            id: row.try_get("id")?,
            code: row.try_get("path")?,
            name: row.try_get("name")?,
            parent_id: row.try_get("parent_id")?,
            kind: read_kind(&kind)?,
            warehouse_id: row.try_get("warehouse_id")?,
            is_replenished: row.try_get("is_replenished")?,
            count_frequency_days: row.try_get("count_frequency_days")?,
            is_active: row.try_get("is_active")?,
        }))
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<LocationSummary> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let kind: String = row.try_get("kind")?;

        Ok(Self(LocationSummary {
            id: row.try_get("id")?,
            code: row.try_get("path")?,
            name: row.try_get("name")?,
            parent_id: row.try_get("parent_id")?,
            kind: read_kind(&kind)?,
            warehouse_id: row.try_get("warehouse_id")?,
            warehouse_name: row.try_get("warehouse_name")?,
            is_replenished: row.try_get("is_replenished")?,
            is_active: row.try_get("is_active")?,
            // Filled by `in_tree_order`.
            depth: 0,
            child_count: row.try_get("child_count")?,
        }))
    }
}

/// Every location, arranged into the tree.
///
/// Inactive rows included: hiding them would hide the zone that live bins sit
/// under, and a tree with a hole in it is unreadable.
pub async fn list<'e, E>(executor: E) -> Result<Vec<LocationSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<LocationSummary>>(
        // LEFT: a counterpart location belongs to no building, which is
        // ordinary rather than a gap.
        "SELECT l.id, l.path, l.name, l.parent_id, l.kind, l.warehouse_id,
                l.is_replenished, l.is_active,
                w.name AS warehouse_name,
                (SELECT count(*) FROM inventory.locations c WHERE c.parent_id = l.id)
                    AS child_count
           FROM inventory.locations l
           LEFT JOIN inventory.warehouses w ON w.id = l.warehouse_id
          ORDER BY l.path",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(app_inventory::location::in_tree_order(
        rows.into_iter().map(|row| row.0).collect(),
    ))
}

/// The kind filter: a kind's own name, or `on_hand`.
pub const KIND: &str = "kind";

/// The status filter: `active` or `inactive`.
pub const STATUS: &str = "status";

const SORTABLE: &[Sortable] = &[
    ("name", "l.name"),
    ("path", "l.path"),
    ("kind", "l.kind"),
    ("is_active", "l.is_active"),
];

/// Tree order and depth, both read off the stored path.
///
/// Arrays compare element by element, so a node's descendants sort immediately
/// after it whatever the collation does to the separator - which ordering by
/// the path as text does not guarantee. A name cannot contain one; see
/// `LocationError::NameHasSeparator`.
const TREE_KEY: &str = "string_to_array(l.path, '/')";

const FROM: &str = "FROM inventory.locations l
           LEFT JOIN inventory.warehouses w ON w.id = l.warehouse_id";

/// A filter nobody set is a NULL that discards its own line.
const WHERE: &str = "WHERE ($1::text IS NULL OR l.kind = $1)
            AND ($2::bool IS NULL OR l.is_active = $2)
            AND ($3::text IS NULL
                 OR l.name ILIKE $3
                 OR l.path ILIKE $3
                 OR w.name ILIKE $3)";

/// One page of the tree, in tree order unless a column was sorted on.
///
/// Depth comes off the path rather than a walk of the parent chain, which is
/// what lets a page be drawn without the rows above it: the indent of a row is
/// a fact about that row. Inactive rows are included for the reason [`list`]
/// includes them.
pub async fn page(
    pool: &sqlx::PgPool,
    request: &PageRequest,
) -> Result<Page<LocationSummary>, DbError> {
    let request = request.sanitised();
    let needle = request
        .needle()
        .map(|needle| crate::search::contains(&needle));

    // `on_hand` is the one choice that is not a kind: it is the question the
    // screen is opened to answer, and `is_on_hand` names the kind that answers.
    let kind = match request.filter(KIND) {
        Some("on_hand") => Some(LocationKind::Internal.as_str()),
        other => other,
    };

    let active = match request.filter(STATUS) {
        Some("active") => Some(true),
        Some("inactive") => Some(false),
        _ => None,
    };

    let counting = AssertSqlSafe(format!("SELECT count(*) {FROM} {WHERE}"));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(kind)
        .bind(active)
        .bind(needle.as_deref())
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    let order = listing::order_by(request.sort.as_ref(), SORTABLE, TREE_KEY);

    let selecting = AssertSqlSafe(format!(
        "SELECT l.id, l.path, l.name, l.parent_id, l.kind, l.warehouse_id,
                l.is_replenished, l.is_active,
                w.name AS warehouse_name,
                cardinality({TREE_KEY}) - 1 AS depth,
                (SELECT count(*) FROM inventory.locations c WHERE c.parent_id = l.id)
                    AS child_count
           {FROM}
           {WHERE}
          ORDER BY {order}, l.path
          LIMIT $4 OFFSET $5"
    ));

    let rows = sqlx::query(selecting)
        .bind(kind)
        .bind(active)
        .bind(needle.as_deref())
        .bind(request.limit() as i64)
        .bind(request.offset() as i64)
        .fetch_all(pool)
        .await
        .map_err(DbError::Query)?;

    let summaries = rows
        .into_iter()
        .map(|row| {
            let mut summary = RowOf::<LocationSummary>::from_row(&row)?.0;
            let depth: i32 = row.try_get("depth")?;
            summary.depth = u16::try_from(depth).unwrap_or(0);
            Ok(summary)
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)?;

    Ok(Page::new(summaries, total, &request))
}

/// The locations a movement may name: active, and not a grouping.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<Location>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<Location>>(
        "SELECT id, path, name, parent_id, kind, warehouse_id, is_replenished,
                count_frequency_days, is_active
           FROM inventory.locations
          WHERE is_active AND kind <> 'view'
          ORDER BY path",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// The one location of a counterpart kind, for the code that has to find "the
/// vendor location" without anybody having chosen one.
///
/// `LIMIT 1` on a path sort rather than a unique constraint: a workspace may
/// have made a second, and the oldest by path is a stable answer.
pub async fn counterpart<'e, E>(
    executor: E,
    kind: LocationKind,
) -> Result<Option<Location>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Location>>(
        "SELECT id, path, name, parent_id, kind, warehouse_id, is_replenished,
                count_frequency_days, is_active
           FROM inventory.locations
          WHERE kind = $1 AND is_active
          ORDER BY path
          LIMIT 1",
    )
    .bind(kind.as_str())
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Location>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Location>>(
        "SELECT id, path, name, parent_id, kind, warehouse_id, is_replenished,
                count_frequency_days, is_active
           FROM inventory.locations
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// Create one. The service works out `path` from the parent's, which is why it
/// is a parameter here rather than something this builds.
pub async fn insert<'e, E>(
    executor: E,
    path: &str,
    draft: &LocationInput,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO inventory.locations
             (path, name, parent_id, kind, warehouse_id, is_replenished,
              count_frequency_days, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
         RETURNING id",
    )
    .bind(path)
    .bind(&draft.name)
    .bind(draft.parent_id)
    .bind(draft.kind.as_str())
    .bind(draft.warehouse_id)
    .bind(draft.is_replenished)
    .bind(draft.count_frequency_days)
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
    draft: &LocationInput,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.locations
            SET path                 = $2,
                name                 = $3,
                parent_id            = $4,
                kind                 = $5,
                warehouse_id         = $6,
                is_replenished       = $7,
                count_frequency_days = $8,
                is_active            = $9,
                updated_at           = now(),
                updated_by           = $10
          WHERE id = $1",
    )
    .bind(id)
    .bind(path)
    .bind(&draft.name)
    .bind(draft.parent_id)
    .bind(draft.kind.as_str())
    .bind(draft.warehouse_id)
    .bind(draft.is_replenished)
    .bind(draft.count_frequency_days)
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_conflict(err, path))?;

    Ok(result.rows_affected() > 0)
}

/// Rewrite the stored paths beneath a node that was renamed or moved.
///
/// One statement over a recursive walk rather than a query per descendant: a
/// warehouse rename touches every bin in the building, and doing that a row at
/// a time is what makes a rename feel broken.
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
        // `path LIKE old || '/%'` rather than a recursive CTE: the path IS the
        // ancestry, which is the whole reason it is stored. The node itself is
        // excluded - its own row was written by the update that called this.
        "UPDATE inventory.locations
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

/// Every id beneath this one, for the re-parent cycle check.
///
/// `UNION`, not `UNION ALL`, so already-cyclic data terminates.
pub async fn descendants<'e, E>(executor: E, id: Uuid) -> Result<Vec<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "WITH RECURSIVE below AS (
             SELECT id FROM inventory.locations WHERE parent_id = $1
             UNION
             SELECT l.id FROM inventory.locations l JOIN below b ON l.parent_id = b.id
         )
         SELECT id FROM below",
    )
    .bind(id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)
}

/// Remove one. Children are `ON DELETE RESTRICT`, so Postgres refuses a node
/// with anything under it; the service checks for movements.
pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM inventory.locations WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}
