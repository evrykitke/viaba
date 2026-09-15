//! `inventory.stock_moves`: the ledger every quantity is answerable to.
//!
//! # Nothing here updates a move that is done
//!
//! The trigger in `0004_stock.sql` refuses it, and this module never tries. The
//! one write against a finished row is [`record_journal`], which fills in where
//! the posting landed - and posting happens inside the same transaction as the
//! move, so it is the same act rather than a later edit.
//!
//! # A move is written, then the quants are moved
//!
//! Both in the caller's transaction. The order matters only for reading a
//! stack trace; what matters is that a failure anywhere leaves neither, which
//! is why every function here takes a connection and none takes a pool.

use app_inventory::category::{CostingMethod, RemovalStrategy, Valuation};
use app_inventory::item::Tracking;
use app_inventory::location::{LocationKind, MoveKind};
use app_inventory::movement::{
    JournalOutcome, MoveContext, MoveFilter, MoveSource, MoveState, MoveSummary, StockMove,
};
use app_inventory::quantity::Quantity;
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::query::{Page, PageRequest};
use sqlx::{AssertSqlSafe, PgConnection, PgExecutor, PgPool, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::listing::{self, Sortable};

/// What a service hands over to write one.
///
/// The costed form of `app_inventory::movement::MoveRequest`: everything the
/// document asked for, plus the three answers only the server has - the stock
/// unit, what a unit cost, and what the whole line is worth.
pub struct NewMove<'a> {
    pub variant_id: Uuid,
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub quantity: Quantity,
    pub unit_id: Uuid,
    pub state: MoveState,
    pub moved_on: NaiveDate,
    pub unit_cost: Money,
    pub value: Money,
    pub reference: Option<&'a str>,
    pub source: Option<&'a MoveSource>,
    /// Why the shelf disagreed with the record. `None` for every movement that
    /// is not an adjustment, which is most of them.
    pub adjustment_type_id: Option<Uuid>,
}

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("stock_moves.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_journal(row: &sqlx::postgres::PgRow) -> Result<JournalOutcome, sqlx::Error> {
    let state: String = row.try_get("journal_state")?;
    let id: Option<Uuid> = row.try_get("journal_id")?;
    let number: Option<String> = row.try_get("journal_number")?;

    JournalOutcome::parse(&state, id, number).ok_or_else(|| unknown("journal_state", &state))
}

fn read_move(row: &sqlx::postgres::PgRow, currency: Currency) -> Result<StockMove, sqlx::Error> {
    let quantity: String = row.try_get("quantity")?;
    let unit_cost: String = row.try_get("unit_cost")?;
    let value: String = row.try_get("value")?;
    let state: String = row.try_get("state")?;
    let doc_type: Option<String> = row.try_get("source_doc_type")?;
    let doc_id: Option<Uuid> = row.try_get("source_doc_id")?;

    Ok(StockMove {
        id: row.try_get("id")?,
        variant_id: row.try_get("variant_id")?,
        from_location_id: row.try_get("from_location_id")?,
        to_location_id: row.try_get("to_location_id")?,
        lot_id: row.try_get("lot_id")?,
        quantity: read_quantity(&quantity, "stock_moves.quantity")?,
        unit_id: row.try_get("unit_id")?,
        state: MoveState::parse(&state).ok_or_else(|| unknown("state", &state))?,
        moved_on: row.try_get("moved_on")?,
        unit_cost: read_money(&unit_cost, currency, "stock_moves.unit_cost")?,
        value: read_money(&value, currency, "stock_moves.value")?,
        reference: row.try_get("reference")?,
        source: doc_type
            .zip(doc_id)
            .map(|(doc_type, doc_id)| MoveSource { doc_type, doc_id }),
        journal: read_journal(row)?,
    })
}

const COLUMNS: &str = "id, variant_id, from_location_id, to_location_id, lot_id,
                       quantity::text AS quantity, unit_id, state, moved_on,
                       unit_cost::text AS unit_cost, value::text AS value,
                       reference, source_doc_type, source_doc_id,
                       journal_state, journal_id, journal_number";

/// Everything a movement needs to know about what is being moved.
///
/// One query rather than four, because a receipt of forty lines would otherwise
/// be a hundred and sixty round trips. `cost` is the item's plus this variant's
/// `cost_extra` - the larger size that costs more to make.
pub async fn context<'e, E>(
    executor: E,
    variant_id: Uuid,
    currency: Currency,
) -> Result<Option<MoveContext>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT v.id AS variant_id, v.code AS variant_code,
                i.id AS item_id, i.name AS item_name, i.is_tracked, i.tracking,
                i.uses_expiry, i.stock_unit_id, i.category_id,
                u.code AS stock_unit_code,
                c.costing_method, c.valuation, c.removal_strategy,
                (i.cost + v.cost_extra)::numeric(19, 4)::text AS cost,
                i.cost::text AS item_cost,
                i.sale_price::text AS sale_price
           FROM inventory.item_variants v
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.stock_unit_id
           JOIN inventory.categories c ON c.id = i.category_id
          WHERE v.id = $1",
    )
    .bind(variant_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(|row| {
        let tracking: String = row.try_get("tracking")?;
        let costing: String = row.try_get("costing_method")?;
        let valuation: String = row.try_get("valuation")?;
        let removal: String = row.try_get("removal_strategy")?;
        let cost: String = row.try_get("cost")?;
        let item_cost: String = row.try_get("item_cost")?;
        let sale_price: String = row.try_get("sale_price")?;

        Ok(MoveContext {
            variant_id: row.try_get("variant_id")?,
            variant_code: row.try_get("variant_code")?,
            item_id: row.try_get("item_id")?,
            item_name: row.try_get("item_name")?,
            is_tracked: row.try_get("is_tracked")?,
            tracking: Tracking::parse(&tracking)
                .ok_or_else(|| unknown("items.tracking", &tracking))?,
            uses_expiry: row.try_get("uses_expiry")?,
            stock_unit_id: row.try_get("stock_unit_id")?,
            stock_unit_code: row.try_get("stock_unit_code")?,
            category_id: row.try_get("category_id")?,
            costing_method: CostingMethod::parse(&costing)
                .ok_or_else(|| unknown("categories.costing_method", &costing))?,
            valuation: Valuation::parse(&valuation)
                .ok_or_else(|| unknown("categories.valuation", &valuation))?,
            removal_strategy: RemovalStrategy::parse(&removal)
                .ok_or_else(|| unknown("categories.removal_strategy", &removal))?,
            cost: read_money(&cost, currency, "items.cost")?,
            item_cost: read_money(&item_cost, currency, "items.cost")?,
            sale_price: read_money(&sale_price, currency, "items.sale_price")?,
        })
    })
    .transpose()
    .map_err(DbError::Query)
}

pub async fn insert(
    conn: &mut PgConnection,
    draft: &NewMove<'_>,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.stock_moves
             (variant_id, from_location_id, to_location_id, lot_id, quantity,
              unit_id, state, moved_on, unit_cost, value, reference,
              source_doc_type, source_doc_id, adjustment_type_id, created_by)
          VALUES ($1, $2, $3, $4, $5::numeric, $6, $7, $8, $9::numeric,
                  $10::numeric, $11, $12, $13, $14, $15)
       RETURNING id",
    )
    .bind(draft.variant_id)
    .bind(draft.from_location_id)
    .bind(draft.to_location_id)
    .bind(draft.lot_id)
    .bind(draft.quantity.to_storage_string())
    .bind(draft.unit_id)
    .bind(draft.state.as_str())
    .bind(draft.moved_on)
    .bind(draft.unit_cost.to_storage_string())
    .bind(draft.value.to_storage_string())
    .bind(draft.reference)
    .bind(draft.source.map(|source| source.doc_type.as_str()))
    .bind(draft.source.map(|source| source.doc_id))
    .bind(draft.adjustment_type_id)
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// Write down where the journal for this move landed.
///
/// The one write the append-only trigger lets through on a finished row,
/// because posting happens in the same transaction as the move.
pub async fn record_journal(
    conn: &mut PgConnection,
    id: Uuid,
    outcome: &JournalOutcome,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.stock_moves
            SET journal_state = $2, journal_id = $3, journal_number = $4
          WHERE id = $1",
    )
    .bind(id)
    .bind(outcome.as_str())
    .bind(outcome.journal_id())
    .bind(outcome.number())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// A draft may still be cancelled; a move that is done may not.
///
/// Answers `false` rather than failing where the row is already final, so a
/// screen can say so instead of showing a trigger's words.
pub async fn cancel(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.stock_moves SET state = 'cancelled' WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

pub async fn find<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<StockMove>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM inventory.stock_moves WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(|row| read_move(&row, currency))
        .transpose()
        .map_err(DbError::Query)
}

/// The range key the movement grid declares, and so the pair of filter keys -
/// `moved_from` and `moved_to` - that arrive with a request.
///
/// A constant because it is written in two crates that must agree and do not
/// depend on each other: here, and `ui::table::config::stock_moves`.
pub const MOVED: &str = "moved";

/// The filter key naming what a move amounts to.
pub const KIND: &str = "kind";

/// The filter key naming whether it happened.
pub const STATE: &str = "state";

/// The columns the movement grid may order by.
///
/// A whitelist, not a convenience: `sort.field` arrives from a browser, and the
/// only safe way to put it in an `ORDER BY` is to not put it there at all - to
/// match it against a list of literals this file wrote itself.
const SORTABLE: &[Sortable] = &[
    ("moved_on", "m.moved_on"),
    ("item", "i.name"),
    ("quantity", "m.quantity"),
    ("value", "m.value"),
    ("journal", "m.journal_number"),
];

/// What a summary row is made of.
const SUMMARY: &str = "m.id, m.moved_on, m.variant_id, m.created_at,
                m.quantity::text AS quantity, m.value::text AS value,
                m.state, m.reference,
                m.journal_state, m.journal_id, m.journal_number,
                v.code AS variant_code,
                i.name AS item_name,
                lt.number AS lot_number,
                f.path AS from_path, f.kind AS from_kind,
                t.path AS to_path, t.kind AS to_kind,
                u.code AS unit_code";

/// Everything a row is read from or narrowed by. Shared so the count and the
/// select cannot come to disagree about which rows exist.
const FROM: &str = "FROM inventory.stock_moves m
           JOIN inventory.item_variants v ON v.id = m.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.locations f ON f.id = m.from_location_id
           JOIN inventory.locations t ON t.id = m.to_location_id
           JOIN inventory.units u ON u.id = m.unit_id
           LEFT JOIN inventory.lots lt ON lt.id = m.lot_id";

/// A filter nobody set is a NULL that discards its own line, so one clause
/// serves every combination and nothing is interpolated.
const WHERE: &str = "WHERE ($1::uuid IS NULL OR m.variant_id = $1)
            AND ($2::uuid IS NULL OR v.item_id = $2)
            AND ($3::uuid IS NULL OR m.from_location_id = $3 OR m.to_location_id = $3)
            AND ($4::uuid IS NULL OR m.lot_id = $4)
            AND ($5::text IS NULL
                 OR i.name ILIKE $5
                 OR v.code ILIKE $5
                 OR f.path ILIKE $5
                 OR t.path ILIKE $5
                 OR lt.number ILIKE $5
                 OR m.journal_number ILIKE $5
                 OR m.reference ILIKE $5)
            AND ($6::date IS NULL OR m.moved_on >= $6)
            AND ($7::date IS NULL OR m.moved_on <= $7)
            AND ($8::text IS NULL OR m.state = $8)
            AND ($9::text[] IS NULL OR f.kind || '>' || t.kind = ANY($9::text[]))";

/// One page of the movement grid.
///
/// # Why this is paged in SQL
///
/// Nothing ever deletes from `stock_moves`: a movement that happened is
/// evidence, and the correction for one is another movement. So the list grows
/// for as long as the workspace trades, and there is no number of rows at which
/// fetching all of them stops being wrong - only a date at which it becomes
/// obvious.
///
/// What stood here before was a `LIMIT 500` with nothing paging it, which is
/// not a smaller answer but a different one: the grid said five hundred, the
/// pager agreed, and movement five hundred and one was not in the world. A cap
/// that cannot be paged past is a silent truncation, and a stock ledger is the
/// last place to put one.
///
/// `filter` is what the *screen* is about - one variant, one location, one lot.
/// `request` is what the *viewer* asked for - the search, the page, the kind,
/// the state, the span. The two are separate because one is chosen by the code
/// that opened the grid and the other by whoever is looking at it.
///
/// Two statements - a count and a select - so the page can be pulled back to
/// one that exists before the rows are fetched.
pub async fn page(
    pool: &PgPool,
    filter: &MoveFilter,
    currency: Currency,
    request: &PageRequest,
) -> Result<Page<MoveSummary>, DbError> {
    let request = request.sanitised();
    let needle = request
        .needle()
        .map(|needle| crate::search::contains(&needle));
    let state = request.filter(STATE).and_then(MoveState::parse);

    let moved = request.range(MOVED);
    let from_day = moved.first_day();
    let to_day = moved.last_day();

    // The kind is derived from two location kinds and there is no column to
    // compare, so the pairs that amount to it come from the domain rather than
    // from a truth table written again here. The separator exists only inside
    // this statement.
    let ends = request.filter(KIND).and_then(MoveKind::parse).map(|kind| {
        kind.ends()
            .into_iter()
            .map(|(from, to)| format!("{}>{}", from.as_str(), to.as_str()))
            .collect::<Vec<String>>()
    });

    // `AssertSqlSafe` because these statements are composed rather than
    // written: `SUMMARY`, `FROM` and `WHERE` are constants, and `order` can
    // only be a string this file put in `SORTABLE`. Nothing from a browser
    // reaches the text of the query.
    let counting = AssertSqlSafe(format!("SELECT count(*) {FROM} {WHERE}"));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(filter.variant_id)
        .bind(filter.item_id)
        .bind(filter.location_id)
        .bind(filter.lot_id)
        .bind(needle.as_deref())
        .bind(from_day)
        .bind(to_day)
        .bind(state.map(MoveState::as_str))
        .bind(ends.as_deref())
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    // Newest first, and `created_at` after it whatever the sort: two movements
    // on the same day would otherwise swap places between one page and the
    // next, which shows up as a row that appears twice.
    let order = listing::order_by(request.sort.as_ref(), SORTABLE, "m.moved_on DESC");

    let selecting = AssertSqlSafe(format!(
        "SELECT {SUMMARY}
           {FROM}
           {WHERE}
          ORDER BY {order}, m.created_at DESC
          LIMIT $10 OFFSET $11"
    ));

    let rows = sqlx::query(selecting)
        .bind(filter.variant_id)
        .bind(filter.item_id)
        .bind(filter.location_id)
        .bind(filter.lot_id)
        .bind(needle.as_deref())
        .bind(from_day)
        .bind(to_day)
        .bind(state.map(MoveState::as_str))
        .bind(ends.as_deref())
        .bind(request.limit() as i64)
        .bind(request.offset() as i64)
        .fetch_all(pool)
        .await
        .map_err(DbError::Query)?;

    let summaries = rows
        .iter()
        .map(|row| read_summary(row, currency))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)?;

    Ok(Page::new(summaries, total, &request))
}

/// One grid row.
fn read_summary(
    row: &sqlx::postgres::PgRow,
    currency: Currency,
) -> Result<MoveSummary, sqlx::Error> {
    let quantity: String = row.try_get("quantity")?;
    let value: String = row.try_get("value")?;
    let state: String = row.try_get("state")?;
    let from_kind: String = row.try_get("from_kind")?;
    let to_kind: String = row.try_get("to_kind")?;

    let kind = |raw: &str| LocationKind::parse(raw).ok_or_else(|| unknown("locations.kind", raw));

    Ok(MoveSummary {
        id: row.try_get("id")?,
        moved_on: row.try_get("moved_on")?,
        variant_id: row.try_get("variant_id")?,
        variant_code: row.try_get("variant_code")?,
        item_name: row.try_get("item_name")?,
        lot_number: row.try_get("lot_number")?,
        from_path: row.try_get("from_path")?,
        to_path: row.try_get("to_path")?,
        from_kind: kind(&from_kind)?,
        to_kind: kind(&to_kind)?,
        quantity: read_quantity(&quantity, "stock_moves.quantity")?,
        unit_code: row.try_get("unit_code")?,
        value: read_money(&value, currency, "stock_moves.value")?,
        state: MoveState::parse(&state).ok_or_else(|| unknown("state", &state))?,
        reference: row.try_get("reference")?,
        journal: read_journal(row)?,
    })
}

/// Whether anything has ever moved across this location.
///
/// Asked before deleting one. A location with a movement against it is half of
/// an entry in the audit trail, and deleting it would orphan the other half.
pub async fn location_has_movements<'e, E>(executor: E, location_id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1 FROM inventory.stock_moves
              WHERE from_location_id = $1 OR to_location_id = $1
         )",
    )
    .bind(location_id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)
}

/// Whether anything has ever moved for this item, in any variant.
pub async fn item_has_movements<'e, E>(executor: E, item_id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1
               FROM inventory.stock_moves m
               JOIN inventory.item_variants v ON v.id = m.variant_id
              WHERE v.item_id = $1
         )",
    )
    .bind(item_id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)
}
