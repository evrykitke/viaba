//! `inventory.stock_transfers` and its lines: what left, and what has arrived.
//!
//! # Saving replaces the lines
//!
//! [`save_lines`] deletes and re-inserts, on the same terms as an order's. It
//! is safe for exactly as long as a line accumulates nothing, and the moment
//! one starts to - the despatch - is the moment the document stops being
//! editable.
//!
//! # `despatched` and `received` move by deltas
//!
//! Never set to a total. Two people receiving two pallets of the same line in
//! the same minute both have to count, which an absolute would lose. The CHECK
//! on each column is what refuses one that would take a line past what left.

use app_inventory::quantity::Quantity;
use app_inventory::transfer::{
    CheckedTransferLine, Transfer, TransferLine, TransferState, TransferSummary,
};
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn read_state(raw: &str) -> Result<TransferState, sqlx::Error> {
    TransferState::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("stock_transfers.state holds '{raw}', which this build does not know").into(),
        )
    })
}

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

/// The grid's shape, written once so two screens cannot disagree about it.
///
/// `in_transit` is summed here rather than in the caller because a list of two
/// hundred journeys is one query either way, and doing it in Rust would mean
/// reading every line of every one of them to draw a column.
const SUMMARY_COLUMNS: &str = "
    t.id, t.number, t.state, t.from_path, t.to_path,
    t.planned_on, t.despatched_on, t.reference,
    (SELECT count(*) FROM inventory.stock_transfer_lines l
      WHERE l.transfer_id = t.id) AS line_count,
    COALESCE((SELECT sum(l.despatched - l.received)
                FROM inventory.stock_transfer_lines l
               WHERE l.transfer_id = t.id), 0)::text AS in_transit
";

fn read_summary(row: &sqlx::postgres::PgRow) -> Result<TransferSummary, sqlx::Error> {
    let state: String = row.try_get("state")?;
    let in_transit: String = row.try_get("in_transit")?;

    Ok(TransferSummary {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        state: read_state(&state)?,
        from_path: row.try_get("from_path")?,
        to_path: row.try_get("to_path")?,
        planned_on: row.try_get("planned_on")?,
        despatched_on: row.try_get("despatched_on")?,
        reference: row.try_get("reference")?,
        line_count: row.try_get("line_count")?,
        in_transit: read_quantity(&in_transit, "stock_transfer_lines.despatched")?,
    })
}

/// Every transfer, newest first.
pub async fn list<'e, E>(executor: E) -> Result<Vec<TransferSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.stock_transfers t
          ORDER BY t.planned_on DESC, t.created_at DESC"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(read_summary)
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Journeys with stock still on them. The transit account's own screen.
pub async fn in_transit<'e, E>(executor: E) -> Result<Vec<TransferSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.stock_transfers t
          WHERE t.state = 'in_transit'
          ORDER BY t.despatched_on, t.created_at"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(read_summary)
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The header, without its lines.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Transfer>, DbError>
where
    E: PgExecutor<'e>,
{
    let Some(row) = sqlx::query(
        "SELECT t.id, t.number, t.state,
                t.from_location_id, t.to_location_id, t.transit_location_id,
                t.from_path, t.to_path,
                t.planned_on, t.despatched_on, t.arrived_on,
                t.reference, t.note
           FROM inventory.stock_transfers t
          WHERE t.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let state: String = row.try_get("state").map_err(DbError::Query)?;

    Ok(Some(Transfer {
        id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: read_state(&state).map_err(DbError::Query)?,
        from_location_id: row.try_get("from_location_id").map_err(DbError::Query)?,
        to_location_id: row.try_get("to_location_id").map_err(DbError::Query)?,
        transit_location_id: row.try_get("transit_location_id").map_err(DbError::Query)?,
        from_path: row.try_get("from_path").map_err(DbError::Query)?,
        to_path: row.try_get("to_path").map_err(DbError::Query)?,
        planned_on: row.try_get("planned_on").map_err(DbError::Query)?,
        despatched_on: row.try_get("despatched_on").map_err(DbError::Query)?,
        arrived_on: row.try_get("arrived_on").map_err(DbError::Query)?,
        reference: row.try_get("reference").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        lines: Vec::new(),
    }))
}

pub async fn lines_of<'e, E>(executor: E, transfer_id: Uuid) -> Result<Vec<TransferLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.variant_id, l.lot_id, l.description,
                l.quantity::text AS quantity,
                l.despatched::text AS despatched,
                l.received::text AS received,
                v.code AS variant_code,
                lt.number AS lot_number
           FROM inventory.stock_transfer_lines l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           LEFT JOIN inventory.lots lt ON lt.id = l.lot_id
          WHERE l.transfer_id = $1
          ORDER BY l.line_no",
    )
    .bind(transfer_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let despatched: String = row.try_get("despatched")?;
            let received: String = row.try_get("received")?;

            Ok(TransferLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                lot_id: row.try_get("lot_id")?,
                lot_number: row.try_get("lot_number")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "stock_transfer_lines.quantity")?,
                despatched: read_quantity(&despatched, "stock_transfer_lines.despatched")?,
                received: read_quantity(&received, "stock_transfer_lines.received")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Which movement each line has already made, so a retried despatch or arrival
/// skips what it did last time.
pub async fn moves_of<'e, E>(
    executor: E,
    transfer_id: Uuid,
) -> Result<Vec<(Uuid, Option<Uuid>, Option<Uuid>)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.despatch_move_id, l.arrival_move_id
           FROM inventory.stock_transfer_lines l
          WHERE l.transfer_id = $1
          ORDER BY l.line_no",
    )
    .bind(transfer_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("id")?,
                row.try_get("despatch_move_id")?,
                row.try_get("arrival_move_id")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The two ends and the middle, named for the document. Paths are snapshotted
/// so a list draws without three joins into a recursive tree.
pub struct Ends {
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub transit_location_id: Uuid,
    pub from_path: String,
    pub to_path: String,
}

pub async fn insert(
    conn: &mut PgConnection,
    ends: &Ends,
    planned_on: NaiveDate,
    reference: Option<&str>,
    note: Option<&str>,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.stock_transfers
             (from_location_id, to_location_id, transit_location_id,
              from_path, to_path, planned_on, reference, note,
              created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
       RETURNING id",
    )
    .bind(ends.from_location_id)
    .bind(ends.to_location_id)
    .bind(ends.transit_location_id)
    .bind(&ends.from_path)
    .bind(&ends.to_path)
    .bind(planned_on)
    .bind(reference)
    .bind(note)
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// Answers `false` where the document is no longer a draft, so a screen can say
/// so rather than silently changing nothing.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    ends: &Ends,
    planned_on: NaiveDate,
    reference: Option<&str>,
    note: Option<&str>,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.stock_transfers
            SET from_location_id = $2, to_location_id = $3, transit_location_id = $4,
                from_path = $5, to_path = $6, planned_on = $7,
                reference = $8, note = $9,
                updated_at = now(), updated_by = $10
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(ends.from_location_id)
    .bind(ends.to_location_id)
    .bind(ends.transit_location_id)
    .bind(&ends.from_path)
    .bind(&ends.to_path)
    .bind(planned_on)
    .bind(reference)
    .bind(note)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Replace the lines. Delete-and-reinsert - see the module header.
pub async fn save_lines(
    conn: &mut PgConnection,
    transfer_id: Uuid,
    lines: &[CheckedTransferLine],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.stock_transfer_lines WHERE transfer_id = $1")
        .bind(transfer_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (position, line) in lines.iter().enumerate() {
        let line_no = i32::try_from(position + 1).unwrap_or(i32::MAX);

        sqlx::query(
            "INSERT INTO inventory.stock_transfer_lines
                 (transfer_id, line_no, variant_id, lot_id, description, quantity)
              VALUES ($1, $2, $3, $4, $5, $6::numeric)",
        )
        .bind(transfer_id)
        .bind(line_no)
        .bind(line.variant_id)
        .bind(line.lot_id)
        .bind(&line.description)
        .bind(line.quantity.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Write down the movement that took one line out of the origin, and how much
/// it took. Both in one statement, because a line carrying a movement that its
/// `despatched` does not account for is the state this is arranged to avoid.
pub async fn record_despatch(
    conn: &mut PgConnection,
    line_id: Uuid,
    move_id: Uuid,
    quantity: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.stock_transfer_lines
            SET despatch_move_id = $2, despatched = despatched + $3::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(move_id)
    .bind(quantity.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// The same, for the movement that put it on the destination's shelf.
pub async fn record_arrival(
    conn: &mut PgConnection,
    line_id: Uuid,
    move_id: Uuid,
    quantity: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.stock_transfer_lines
            SET arrival_move_id = $2, received = received + $3::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(move_id)
    .bind(quantity.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Take the number and the date the stock left.
///
/// Answers `false` where somebody else despatched it first, so the caller can
/// stop rather than move the same pallet twice.
pub async fn despatch(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    despatched_on: NaiveDate,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.stock_transfers
            SET state = 'in_transit', number = $2, despatched_on = $3,
                despatched_by = $4, updated_at = now(), updated_by = $4
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(despatched_on)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Close the journey once nothing is left on it.
///
/// The date is kept whether or not the state moves, because a part-arrival is
/// still an arrival and "when did the first pallet get here" is a question
/// somebody asks.
pub async fn arrive(
    conn: &mut PgConnection,
    id: Uuid,
    arrived_on: NaiveDate,
    complete: bool,
    actor: Option<UserId>,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.stock_transfers
            SET state = CASE WHEN $3 THEN 'done' ELSE state END,
                arrived_on = $2, arrived_by = $4,
                updated_at = now(), updated_by = $4
          WHERE id = $1 AND state = 'in_transit'",
    )
    .bind(id)
    .bind(arrived_on)
    .bind(complete)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Only a draft. Stock that has left cannot be un-sent - a load that turned
/// back is received, which is section 6.3's rule and not this function's job.
pub async fn cancel(
    conn: &mut PgConnection,
    id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.stock_transfers
            SET state = 'cancelled', updated_at = now(), updated_by = $2
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done =
        sqlx::query("DELETE FROM inventory.stock_transfers WHERE id = $1 AND state = 'draft'")
            .bind(id)
            .execute(conn)
            .await
            .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}
