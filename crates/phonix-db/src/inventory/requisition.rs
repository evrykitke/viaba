//! `inventory.requisitions` and its lines.
//!
//! # There is no currency column
//!
//! Unlike an order, which carries the supplier's. A requisition names no
//! supplier, so there is nothing to quote in but the workspace's own - and the
//! estimate on a line is advisory in that currency. Every read here therefore
//! takes the workspace currency as an argument rather than reading one off the
//! row, which is the opposite of [`super::purchase`] and for the opposite
//! reason.
//!
//! # Saving a requisition replaces its lines
//!
//! [`save_lines`] deletes and re-inserts rather than diffing, on the same terms
//! as an order's: the form edits the whole thing at once and a diff would be
//! machinery to reproduce what the form already knows. It takes a transaction.
//!
//! Note what that means for `ordered`: re-inserting a line loses the quantity
//! already put on an order. It is safe because only a **draft** may be edited
//! and nothing can be ordered from a draft - the schema and
//! `RequisitionState::is_editable` both say so - but it is the reason
//! [`advance_ordered`] updates a line by id rather than going through here.
//!
//! # Every join here is an inner join
//!
//! A line names a variant and a unit, both NOT NULL, so [`lines_of`] joins
//! rather than left-joins. An earlier version allowed a line that merely
//! described something and had to left-join for it; the rule changed, and the
//! query is the simpler for it.
//!
//! # A decision is written once
//!
//! [`decide`] carries `WHERE state = 'submitted'` in its own text, so a second
//! approval of an already-approved requisition affects no rows rather than
//! overwriting who decided it. That is this codebase's rule for a state change -
//! in the statement, never in a trigger.

use app_inventory::quantity::Quantity;
use app_inventory::requisition::{
    Checked, Decision, Demand, Requisition, RequisitionLine, RequisitionState, RequisitionSummary,
};
use chrono::{DateTime, NaiveDate, Utc};
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_ports::cost_centre::CostCentre;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("requisitions.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

/// The three cost-centre columns, read back as the one thing they are.
///
/// All three are NOT NULL, so there is no half-filled case to handle - see the
/// head of `0007_requisitions.sql` for why the column stopped being optional.
fn read_cost_centre(row: &sqlx::postgres::PgRow) -> Result<CostCentre, sqlx::Error> {
    Ok(CostCentre {
        id: row.try_get("cost_centre_id")?,
        code: row.try_get("cost_centre_code")?,
        name: row.try_get("cost_centre_name")?,
    })
}

/// The summary columns, which four different queries here select identically.
///
/// Written once because they are the grid's shape, and a second copy that drifts
/// is a column that means something different depending on which screen you are
/// on. The estimate is the interesting one: `bool_and` over the lines is what
/// makes a partial total come back NULL rather than misleading, matching
/// `Requisition::estimate`.
const SUMMARY_COLUMNS: &str = "
    r.id, r.number, r.state, r.cost_centre_name, r.raised_on, r.needed_by,
    w.name AS warehouse_name,
    u.display_name AS raised_by_name,
    (SELECT count(*) FROM inventory.requisition_lines l
      WHERE l.requisition_id = r.id) AS line_count,
    COALESCE((
        SELECT CASE
            WHEN bool_and(l.ordered >= l.quantity) THEN 'everything'
            WHEN bool_or(l.ordered > 0) THEN 'partly'
            ELSE 'nothing' END
          FROM inventory.requisition_lines l
         WHERE l.requisition_id = r.id
    ), 'nothing') AS order_progress,
    (SELECT CASE WHEN bool_and(l.estimate IS NOT NULL)
                 THEN sum(round(l.quantity * l.estimate, 4)) END::text
       FROM inventory.requisition_lines l
      WHERE l.requisition_id = r.id) AS estimate
";

fn read_summary(
    row: &sqlx::postgres::PgRow,
    currency: Currency,
) -> Result<RequisitionSummary, sqlx::Error> {
    let state: String = row.try_get("state")?;
    let progress: String = row.try_get("order_progress")?;
    let estimate: Option<String> = row.try_get("estimate")?;

    Ok(RequisitionSummary {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        state: RequisitionState::parse(&state).ok_or_else(|| unknown("state", &state))?,
        order_progress: read_progress(&progress)?,
        cost_centre_name: row.try_get("cost_centre_name")?,
        warehouse_name: row.try_get("warehouse_name")?,
        raised_on: row.try_get("raised_on")?,
        needed_by: row.try_get("needed_by")?,
        raised_by_name: row.try_get("raised_by_name")?,
        line_count: row.try_get("line_count")?,
        estimate: estimate
            .map(|raw| read_money(&raw, currency, "requisition_lines.estimate"))
            .transpose()?,
    })
}

fn read_progress(raw: &str) -> Result<app_inventory::requisition::OrderProgress, sqlx::Error> {
    use app_inventory::requisition::OrderProgress;

    match raw {
        "nothing" => Ok(OrderProgress::Nothing),
        "partly" => Ok(OrderProgress::Partly),
        "everything" => Ok(OrderProgress::Everything),
        other => Err(unknown("order_progress", other)),
    }
}

/// Every requisition, newest first.
pub async fn list<'e, E>(executor: E, currency: Currency) -> Result<Vec<RequisitionSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.requisitions r
           JOIN inventory.warehouses w ON w.id = r.warehouse_id
           LEFT JOIN core.users u ON u.id = r.created_by
          ORDER BY r.raised_on DESC, r.created_at DESC"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| read_summary(row, currency))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What is waiting on somebody. The approver's own screen.
pub async fn awaiting_decision<'e, E>(
    executor: E,
    currency: Currency,
) -> Result<Vec<RequisitionSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.requisitions r
           JOIN inventory.warehouses w ON w.id = r.warehouse_id
           LEFT JOIN core.users u ON u.id = r.created_by
          WHERE r.state = 'submitted'
          ORDER BY COALESCE(r.needed_by, 'infinity'::date), r.raised_on"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| read_summary(row, currency))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<Requisition>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT r.id, r.number, r.state, r.cost_centre_id, r.cost_centre_code,
                r.cost_centre_name, r.warehouse_id, r.raised_on, r.needed_by,
                r.justification, r.note, r.decided_at, r.decided_by, r.decision_note,
                r.created_by,
                w.name AS warehouse_name,
                raiser.display_name AS raised_by_name,
                decider.display_name AS decided_by_name
           FROM inventory.requisitions r
           JOIN inventory.warehouses w ON w.id = r.warehouse_id
           LEFT JOIN core.users raiser ON raiser.id = r.created_by
           LEFT JOIN core.users decider ON decider.id = r.decided_by
          WHERE r.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let state: String = row.try_get("state").map_err(DbError::Query)?;
    let decided_at: Option<DateTime<Utc>> = row.try_get("decided_at").map_err(DbError::Query)?;

    let lines = lines_of(executor, id, currency).await?;

    Ok(Some(Requisition {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: RequisitionState::parse(&state)
            .ok_or_else(|| DbError::Query(unknown("state", &state)))?,
        cost_centre: read_cost_centre(&row).map_err(DbError::Query)?,
        warehouse_id: row.try_get("warehouse_id").map_err(DbError::Query)?,
        warehouse_name: row.try_get("warehouse_name").map_err(DbError::Query)?,
        raised_on: row.try_get("raised_on").map_err(DbError::Query)?,
        needed_by: row.try_get("needed_by").map_err(DbError::Query)?,
        justification: row.try_get("justification").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        decision: decided_at
            .map(|at| -> Result<Decision, sqlx::Error> {
                Ok(Decision {
                    at,
                    by: row.try_get("decided_by")?,
                    by_name: row.try_get("decided_by_name")?,
                    note: row.try_get("decision_note")?,
                })
            })
            .transpose()
            .map_err(DbError::Query)?,
        raised_by: row.try_get("created_by").map_err(DbError::Query)?,
        raised_by_name: row.try_get("raised_by_name").map_err(DbError::Query)?,
        lines,
    }))
}

pub async fn lines_of<'e, E>(
    executor: E,
    requisition_id: Uuid,
    currency: Currency,
) -> Result<Vec<RequisitionLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.variant_id, l.description,
                l.quantity::text AS quantity, l.unit_id,
                l.ordered::text AS ordered, l.estimate::text AS estimate, l.note,
                v.code AS variant_code,
                u.code AS unit_code
           FROM inventory.requisition_lines l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.units u ON u.id = l.unit_id
          WHERE l.requisition_id = $1
          ORDER BY l.line_no",
    )
    .bind(requisition_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let ordered: String = row.try_get("ordered")?;
            let estimate: Option<String> = row.try_get("estimate")?;

            Ok(RequisitionLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "requisition_lines.quantity")?,
                unit_id: row.try_get("unit_id")?,
                unit_code: row.try_get("unit_code")?,
                ordered: read_quantity(&ordered, "requisition_lines.ordered")?,
                estimate: estimate
                    .map(|raw| read_money(&raw, currency, "requisition_lines.estimate"))
                    .transpose()?,
                note: row.try_get("note")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The header, on create.
pub async fn insert(
    conn: &mut PgConnection,
    draft: &Checked,
    centre: &CostCentre,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.requisitions
             (cost_centre_id, cost_centre_code, cost_centre_name, warehouse_id,
              raised_on, needed_by, justification, note, created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
       RETURNING id",
    )
    .bind(centre.id)
    .bind(centre.code.as_str())
    .bind(centre.name.as_str())
    .bind(draft.warehouse_id)
    .bind(draft.raised_on)
    .bind(draft.needed_by)
    .bind(draft.justification.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// The header, on edit. Answers `false` where the row is gone or is no longer a
/// draft - which is the same answer, because both mean this edit does not apply.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &Checked,
    centre: &CostCentre,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.requisitions
            SET cost_centre_id = $2, cost_centre_code = $3, cost_centre_name = $4,
                warehouse_id = $5, raised_on = $6, needed_by = $7,
                justification = $8, note = $9,
                updated_at = now(), updated_by = $10
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(centre.id)
    .bind(centre.code.as_str())
    .bind(centre.name.as_str())
    .bind(draft.warehouse_id)
    .bind(draft.raised_on)
    .bind(draft.needed_by)
    .bind(draft.justification.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// One line, as the service worked it out: the estimate parsed against the
/// workspace's currency, and everything else already checked.
pub struct EstimatedLine<'a> {
    pub source: &'a app_inventory::requisition::CheckedLine,
    pub estimate: Option<Money>,
    /// What the line is called on the request. The item's name where one was
    /// named and the requester typed nothing, and their own words otherwise.
    pub description: &'a str,
}

/// Replace a requisition's lines with what the form holds.
///
/// Safe to lose `ordered` here - see the module header.
pub async fn save_lines(
    conn: &mut PgConnection,
    requisition_id: Uuid,
    lines: &[EstimatedLine<'_>],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.requisition_lines WHERE requisition_id = $1")
        .bind(requisition_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (index, line) in lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory.requisition_lines
                 (requisition_id, line_no, variant_id, description, quantity,
                  unit_id, estimate, note)
              VALUES ($1, $2, $3, $4, $5::numeric, $6, $7::numeric, $8)",
        )
        .bind(requisition_id)
        .bind(index as i32 + 1)
        .bind(line.source.variant_id)
        .bind(line.description)
        .bind(line.source.quantity.to_storage_string())
        .bind(line.source.unit_id)
        .bind(line.estimate.map(|amount| amount.to_storage_string()))
        .bind(line.source.note.as_deref())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Take a number and become a question somebody has to answer.
///
/// `WHERE state = 'draft'` in the statement, so submitting twice allocates one
/// number rather than two. The caller has already drawn the number from the
/// series; a `false` here means it drew one for a row that had moved on, which
/// the service turns into a rolled-back transaction and therefore an unspent
/// number.
pub async fn submit(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.requisitions
            SET state = 'submitted', number = $2,
                submitted_at = now(), submitted_by = $3,
                updated_at = now(), updated_by = $3
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Approve or reject. Answers `false` where somebody else got there first.
pub async fn decide(
    conn: &mut PgConnection,
    id: Uuid,
    state: RequisitionState,
    note: &str,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.requisitions
            SET state = $2, decided_at = now(), decided_by = $3, decision_note = $4,
                updated_at = now(), updated_by = $3
          WHERE id = $1 AND state = 'submitted'",
    )
    .bind(id)
    .bind(state.as_str())
    .bind(actor)
    .bind(note)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Withdraw one. Only from the two states where nobody has committed anything.
pub async fn cancel(
    conn: &mut PgConnection,
    id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.requisitions
            SET state = 'cancelled', updated_at = now(), updated_by = $2
          WHERE id = $1 AND state IN ('draft', 'submitted')",
    )
    .bind(id)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Record that some of a line has reached a purchase order.
///
/// By line id and additive, which is what makes one requisition satisfiable by
/// two orders. The schema's `requisition_lines_ordered_within_request` is the
/// backstop: a consolidation that tried to order more than was asked for is
/// refused by the row rather than quietly stored.
pub async fn advance_ordered(
    conn: &mut PgConnection,
    line_id: Uuid,
    quantity: Quantity,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.requisition_lines
            SET ordered = ordered + $2::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(quantity.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// A draft, thrown away. Lines go with it, by `ON DELETE CASCADE`.
///
/// Only a draft: once it has a number somebody has been quoted it, and the way
/// to end a numbered requisition is [`cancel`], which keeps the row.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query("DELETE FROM inventory.requisitions WHERE id = $1 AND state = 'draft'")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// What approved requisitions are still waiting for, grouped by item and place.
///
/// The consolidation screen's whole query. Reads `requisition_demand`, where
/// the "approved only, something still outstanding" rule lives - see
/// `migrations/apps/inventory/0007_requisitions.sql`.
pub async fn demand<'e, E>(executor: E) -> Result<Vec<Demand>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT d.variant_id, d.warehouse_id, d.lines, d.requisitions,
                d.outstanding::text AS outstanding, d.needed_by, d.oldest_request,
                v.code AS variant_code,
                i.name AS item_name,
                u.code AS unit_code,
                w.name AS warehouse_name
           FROM inventory.requisition_demand d
           JOIN inventory.item_variants v ON v.id = d.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.stock_unit_id
           JOIN inventory.warehouses w ON w.id = d.warehouse_id
          ORDER BY COALESCE(d.needed_by, 'infinity'::date), d.oldest_request, i.name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let outstanding: String = row.try_get("outstanding")?;

            Ok(Demand {
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                item_name: row.try_get("item_name")?,
                warehouse_id: row.try_get("warehouse_id")?,
                warehouse_name: row.try_get("warehouse_name")?,
                unit_code: row.try_get("unit_code")?,
                requisitions: row.try_get("requisitions")?,
                lines: row.try_get("lines")?,
                outstanding: read_quantity(&outstanding, "requisition_demand.outstanding")?,
                needed_by: row.try_get::<Option<NaiveDate>, _>("needed_by")?,
                oldest_request: row.try_get("oldest_request")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The outstanding lines behind one item's demand, oldest request first.
///
/// What consolidation actually consumes: [`demand`] says how much to buy, this
/// says whose request each unit came from, and the order is FIFO by request date
/// because the department that has waited longest should be the one served by
/// the first delivery.
pub async fn outstanding_lines_for<'e, E>(
    executor: E,
    variant_id: Uuid,
    warehouse_id: Uuid,
) -> Result<Vec<(Uuid, Uuid, Quantity)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, r.id AS requisition_id,
                (l.quantity - l.ordered)::text AS outstanding
           FROM inventory.requisition_lines l
           JOIN inventory.requisitions r ON r.id = l.requisition_id
          WHERE r.state = 'approved'
            AND r.warehouse_id = $2
            AND l.variant_id = $1
            AND l.ordered < l.quantity
          ORDER BY r.raised_on, r.created_at, l.line_no",
    )
    .bind(variant_id)
    .bind(warehouse_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let outstanding: String = row.try_get("outstanding")?;

            Ok((
                row.try_get("id")?,
                row.try_get("requisition_id")?,
                read_quantity(&outstanding, "requisition_lines.outstanding")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}
