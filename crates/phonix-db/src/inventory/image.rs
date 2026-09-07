//! `inventory.item_images`: the pictures a grid and a till are made of.

use app_inventory::image::{Gallery, Image, ImageInput};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// Every picture an item has, its own and its variants'.
///
/// One query for the whole gallery so choosing a tile is a lookup rather than a
/// query per row - which is what a till page of forty tiles would otherwise be.
pub async fn gallery<'e, E>(executor: E, item_id: Uuid) -> Result<Gallery, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, item_id, variant_id, file_id, alt_text, position
           FROM inventory.item_images
          WHERE item_id = $1
          ORDER BY position, id",
    )
    .bind(item_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let images = rows
        .iter()
        .map(|row| {
            Ok(Image {
                id: row.try_get("id").map_err(DbError::Query)?,
                item_id: row.try_get("item_id").map_err(DbError::Query)?,
                variant_id: row.try_get("variant_id").map_err(DbError::Query)?,
                file_id: row.try_get("file_id").map_err(DbError::Query)?,
                alt_text: row.try_get("alt_text").map_err(DbError::Query)?,
                position: row.try_get("position").map_err(DbError::Query)?,
            })
        })
        .collect::<Result<Vec<Image>, DbError>>()?;

    Ok(Gallery::new(images))
}

/// The one picture each of a set of items shows, for a grid.
///
/// One query for the whole page rather than a gallery per row. `DISTINCT ON`
/// takes the first by position, which is the same choice `Gallery::tile` makes -
/// deliberately, so a grid tile and a detail screen show the same photograph.
pub async fn tiles<'e, E>(executor: E, item_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>, DbError>
where
    E: PgExecutor<'e>,
{
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT DISTINCT ON (item_id) item_id, file_id
           FROM inventory.item_images
          WHERE item_id = ANY($1)
          ORDER BY item_id, variant_id NULLS FIRST, position, id",
    )
    .bind(item_ids)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("item_id").map_err(DbError::Query)?,
                row.try_get("file_id").map_err(DbError::Query)?,
            ))
        })
        .collect()
}

pub async fn attach<'e, E>(
    executor: E,
    draft: &ImageInput,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO inventory.item_images
             (item_id, variant_id, file_id, alt_text, position, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(draft.item_id)
    .bind(draft.variant_id)
    .bind(draft.file_id)
    .bind((!draft.alt_text.is_empty()).then_some(draft.alt_text.as_str()))
    .bind(draft.position)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db)
            if db.constraint() == Some("item_images_item_file")
                || db.constraint() == Some("item_images_variant_file") =>
        {
            DbError::CodeExists {
                entity: "item_image",
                code: draft.file_id.to_string(),
            }
        }
        _ => DbError::Query(err),
    })
}

/// Move a picture in the gallery.
pub async fn reorder<'e, E>(executor: E, id: Uuid, position: i32) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("UPDATE inventory.item_images SET position = $2 WHERE id = $1")
        .bind(id)
        .bind(position.max(0))
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// Take a picture off an item.
///
/// The stored file itself stays. It may be on another item, and deleting it
/// here would be this screen reaching into the file store.
pub async fn detach<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM inventory.item_images WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}
