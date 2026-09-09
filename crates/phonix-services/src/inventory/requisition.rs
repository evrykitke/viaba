//! Requisitions: asking, deciding, and what happens to a request afterwards.
//!
//! # Nothing here touches the `Ledger` port
//!
//! Not because it was forgotten. A requisition commits the workspace to
//! nothing, nothing has arrived and nothing is owed, so there is no journal to
//! post - the accounting on this chain starts at the receipt. See
//! [`super::purchase`], which says the same thing one document later.
//!
//! # The cost centre is resolved, not trusted
//!
//! The form hands over an id. That id is checked against the `CostCentres` port
//! before it is stored, and what gets stored is the *port's* answer - the code
//! and the name beside the id, snapshotted, so a department renamed next year
//! does not rewrite a request somebody approved. An id the port does not know
//! is refused rather than saved, which is the same rule Books applies to a
//! journal line's cost centre.
//!
//! The cost centre is **required**, so an absent provider - a workspace with no
//! HR app - is a workspace that cannot raise a requisition at all. That is a
//! real dependency between two apps, taken knowingly; see the module docs of
//! [`app_inventory::requisition`] for the reasoning and
//! `0007_requisitions.sql` for the column.
//!
//! # Both answers carry a reason
//!
//! [`decide`] takes a note that has to be there whichever way the answer goes.
//! An earlier version required one only on a rejection; that was overruled.
//!
//! # Submitting allocates the number; approving does not
//!
//! The number belongs to the act of asking, not to the answer. A rejected
//! requisition keeps the number it was quoted under, which is why
//! [`decide`] never touches the column.

use app_inventory::requisition::{
    Checked, DecisionInput, Demand, Requisition, RequisitionError, RequisitionInput,
    RequisitionState, RequisitionSummary,
};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::requisition as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::CostCentres;
use phonix_ports::cost_centre::CostCentre;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<RequisitionSummary>> {
    caller.require(permissions::REQUISITIONS)?;

    let currency = base_currency(pool).await?;
    Ok(store::list(pool, currency).await?)
}

/// What is waiting on a decision. The approver's own screen.
///
/// Gated on `DECIDE` rather than on `REQUISITIONS`: this list is a queue of work
/// for whoever answers them, and showing it to somebody who cannot answer is a
/// screen with nothing on it that works.
pub async fn awaiting_decision(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<RequisitionSummary>> {
    caller.require(permissions::REQUISITIONS_DECIDE)?;

    let currency = base_currency(pool).await?;
    Ok(store::awaiting_decision(pool, currency).await?)
}

/// The cost centres a requisition may be charged to.
///
/// Empty is a real answer: a workspace without the HR app has none, and the form
/// says so rather than refusing to open.
pub async fn chargeable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<CostCentre>> {
    caller.require(permissions::REQUISITIONS)?;

    let centres = crate::hr::HrCostCentres::new(pool.clone());

    centres.list().await.map_err(from_port)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Requisition> {
    caller.require(permissions::REQUISITIONS)?;

    let currency = base_currency(pool).await?;

    store::find(pool, id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("requisition", msg!("requisitions.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<RequisitionInput> {
    Ok(RequisitionInput::from_requisition(
        &detail(pool, caller, id).await?,
    ))
}

/// A blank requisition, on today's date.
///
/// No pool, unlike an order's: an order opens on the workspace's currency and
/// has to read the profile for it. A requisition names no currency at all.
pub fn blank(caller: &Caller) -> ServiceResult<RequisitionInput> {
    caller.require(permissions::REQUISITIONS_CREATE)?;
    Ok(RequisitionInput::blank(today()))
}

/// Write a requisition, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: RequisitionInput,
) -> ServiceResult<Submission<RequisitionInput>> {
    caller.require(permissions::REQUISITIONS_CREATE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let currency = base_currency(pool).await?;

    let centre = match resolve_centre(pool, checked.cost_centre_id).await? {
        Ok(centre) => centre,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // A draft only. Read before anything is written, because `save_lines`
    // replaces the lines and an edit of a submitted requisition would be an
    // approval attached to a question that has since changed.
    if let Some(id) = checked.id {
        let before = detail(pool, caller, id).await?;

        if !before.state.is_editable() {
            return Ok(Submission::rejected(
                "state",
                RequisitionError::NotEditable.message(),
            ));
        }
    }

    let lines = match price_lines(&checked, currency) {
        Ok(lines) => lines,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => store::insert(&mut tx, &checked, &centre, caller.user_id()).await?,
        Some(id) => {
            if !store::update(&mut tx, id, &checked, &centre, caller.user_id()).await? {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "state",
                    RequisitionError::NotEditable.message(),
                ));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &lines).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = RequisitionInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::REQUISITION, id)
        .named(&centre.label())
        .fact("lines", &lines.len().to_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Ask. Allocates the number and puts it in front of whoever answers.
pub async fn submit(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<Requisition>> {
    caller.require(permissions::REQUISITIONS_CREATE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !before.state.is_editable() {
        return Ok(Submission::rejected(
            "state",
            RequisitionError::NotEditable.message(),
        ));
    }
    if before.lines.is_empty() {
        return Ok(Submission::rejected(
            "lines",
            RequisitionError::NoLines.message(),
        ));
    }

    // Outside the transaction, for the reason `purchase::confirm` gives: every
    // requisition queues through the sequence's one row, so anything that can
    // happen before that lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::REQUISITION);
    let allocated = match generator.next(&mut tx, key, before.raised_on).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "number",
                msg!("requisitions.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    if !store::submit(&mut tx, id, &allocated.number, caller.user_id()).await? {
        // Somebody submitted it between the read and the write. Rolling back
        // returns the number rather than leaving a hole in the series.
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "state",
            RequisitionError::NotEditable.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::REQUISITION, id).named(&stored.number),
        &before,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Answer one, either way.
///
/// One function rather than `approve` and `reject`, because they now differ in
/// exactly one place - the state written. The note is required whichever way it
/// goes, so there is nothing else for a second function to get wrong.
pub async fn decide(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    approving: bool,
    decision: DecisionInput,
) -> ServiceResult<Submission<Requisition>> {
    caller.require(permissions::REQUISITIONS_DECIDE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !before.state.awaits_decision() {
        return Ok(Submission::rejected(
            "state",
            RequisitionError::NotDecidable.message(),
        ));
    }

    let note = match decision.check() {
        Ok(note) => note,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let state = if approving {
        RequisitionState::Approved
    } else {
        RequisitionState::Rejected
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if !store::decide(&mut tx, id, state, &note, caller.user_id()).await? {
        // Somebody else answered it first. Their decision stands - overwriting
        // it would replace a named decider with another one, silently.
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "state",
            RequisitionError::NotDecidable.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::REQUISITION, id)
            .named(&stored.number)
            .fact("decision", state.as_str()),
        &before,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Withdraw one. Only the person's own side of the conversation - a requisition
/// nobody has answered yet.
pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::REQUISITIONS_CREATE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if !store::cancel(&mut tx, id, caller.user_id()).await? {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "state",
            RequisitionError::NotDecidable.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let after = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::REQUISITION, id).named(&after.label()),
        &before,
        &after,
    )
    .await;

    Ok(Submission::Saved(()))
}

/// Throw a draft away.
///
/// A draft only: once it has a number somebody has been quoted it, and the way
/// to end a numbered requisition is [`cancel`], which keeps the row.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::REQUISITIONS_CREATE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !before.state.is_editable() {
        return Ok(false);
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::REQUISITION, id).named(&before.label()),
            &before,
        )
        .await;
    }

    Ok(removed)
}

/// What approved requisitions are still waiting for, grouped for consolidation.
///
/// Gated on the order permission rather than the requisition one: this is the
/// buyer's screen, and what it is for is turning it into an order.
pub async fn demand(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Demand>> {
    caller.require(permissions::PURCHASE_ORDERS_CREATE)?;
    Ok(store::demand(pool).await?)
}

// ---------------------------------------------------------------------------
// Working out
// ---------------------------------------------------------------------------

/// Check the cost centre against the port, and take its answer.
///
/// What gets stored is the port's own `CostCentre`, never the id the form sent
/// with a name the browser supplied: the snapshot has to be the provider's
/// spelling or it is a second copy of the department that can disagree.
///
/// There is no `None` case any more. `RequisitionInput::check` has already
/// refused a draft with no cost centre on it, so by here there is an id, and an
/// id the port does not recognise is a rejection rather than a blank.
async fn resolve_centre(
    pool: &PgPool,
    id: Uuid,
) -> ServiceResult<Result<CostCentre, RequisitionError>> {
    let centres = crate::hr::HrCostCentres::new(pool.clone());

    match centres.resolve(id).await {
        Ok(Some(centre)) => Ok(Ok(centre)),
        // The port answered, and the answer is that it does not know this id.
        // A form offering a stale list is the ordinary way to get here; so is a
        // workspace that switched the HR app off since the draft was written.
        Ok(None) => Ok(Err(RequisitionError::UnknownCostCentre)),
        Err(err) => Err(from_port(err)),
    }
}

/// What a port failure means here.
///
/// Written out rather than shared with Books', because the field it lands
/// beside differs and that field is the whole value of the mapping: `Refused`
/// belongs against the picker that offered the id, and `Unavailable` must fail
/// the save rather than be read as "this workspace has no cost centres" - which
/// would quietly store a requisition nobody can charge.
fn from_port(err: phonix_ports::PortError) -> ServiceError {
    match err {
        phonix_ports::PortError::Refused(message) => {
            ServiceError::rejected("cost_centre_id", message)
        }
        unavailable @ phonix_ports::PortError::Unavailable { .. } => {
            tracing::error!(error = %unavailable, "the cost centre port failed");
            ServiceError::rejected("cost_centre_id", msg!("requisitions.error.port_unavailable"))
        }
    }
}

/// Parse each line's estimate against the workspace's own currency.
///
/// The one thing `RequisitionInput::check` could not do, for the reason an
/// order's unit price is left as text: the currency is not the domain's to know.
fn price_lines<'a>(
    checked: &'a Checked,
    currency: Currency,
) -> Result<Vec<phonix_db::inventory::requisition::EstimatedLine<'a>>, RequisitionError> {
    checked
        .lines
        .iter()
        .map(|line| {
            let estimate = if line.estimate.is_empty() {
                None
            } else {
                Some(Money::parse(currency, &line.estimate)?)
            };

            Ok(phonix_db::inventory::requisition::EstimatedLine {
                source: line,
                estimate,
                description: line.description.as_str(),
            })
        })
        .collect()
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
