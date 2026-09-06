//! Whether this workspace has what an app needs.
//!
//! Each app declares its items as a `&'static [SetupItem]`; this answers them
//! against the tenant database. The declaration is in the app crate and the
//! predicate is here for the reason `apps::CATALOG` and `tenancy::apps` are
//! split: one compiles to wasm and the other holds a pool.
//!
//! See `docs/adr/0006-apps-ports-and-defaults.md` section 4.

use phonix_core::i18n::Message;
use phonix_core::setup::{self, SetupItem, SetupStatus};
use phonix_core::{apps, msg, pmsg};
use phonix_db::sqlx::PgPool;

use crate::caller::Caller;
use crate::error::{ServiceError, ServiceResult};

/// The checklist for one app, as its home page draws it.
pub async fn checklist(
    pool: &PgPool,
    caller: &Caller,
    app_id: &str,
) -> ServiceResult<Vec<SetupStatus>> {
    let app = apps::find(app_id)
        .ok_or_else(|| ServiceError::rejected("app", msg!("apps.error.unknown")))?;

    caller.require(app.permission)?;

    answer(pool, app_id).await
}

/// Refuse while anything blocking is undone, naming the first thing missing.
///
/// What a posting calls. An advisory item never reaches here: the difference
/// between the two kinds is exactly this function.
pub async fn require_ready(pool: &PgPool, app_id: &str) -> ServiceResult<()> {
    let statuses = answer(pool, app_id).await?;

    match setup::gaps(&statuses).first() {
        None => Ok(()),
        Some(gap) => Err(ServiceError::rejected(
            "setup",
            gap.note.clone().unwrap_or_else(|| gap.label.clone()),
        )),
    }
}

/// An app with no declared items is set up by definition, which is why an
/// unknown id is an empty list rather than a fault: `checklist` has already
/// refused one that is not in the catalog.
async fn answer(pool: &PgPool, app_id: &str) -> ServiceResult<Vec<SetupStatus>> {
    match app_id {
        app_books::APP_ID => books(pool).await,
        app_hr::APP_ID => hr(pool).await,
        _ => Ok(Vec::new()),
    }
}

async fn books(pool: &PgPool) -> ServiceResult<Vec<SetupStatus>> {
    let (accounts, _active) = phonix_db::books::account::counts(pool).await?;
    let (_periods, open_periods) = phonix_db::books::period::counts(pool).await?;
    let taxes = phonix_db::master::tax::list_codes(pool).await?.len() as i64;

    Ok(vec![
        answered(
            &app_books::SETUP[0],
            accounts > 0,
            pmsg!("books.setup.chart_found", accounts),
        ),
        answered(
            &app_books::SETUP[1],
            open_periods > 0,
            pmsg!("books.setup.periods_found", open_periods),
        ),
        answered(
            &app_books::SETUP[2],
            taxes > 0,
            pmsg!("books.setup.taxes_found", taxes),
        ),
    ])
}

async fn hr(pool: &PgPool) -> ServiceResult<Vec<SetupStatus>> {
    let (_total, chargeable) = phonix_db::hr::department::counts(pool).await?;

    Ok(vec![answered(
        &app_hr::SETUP[0],
        chargeable > 0,
        pmsg!("hr.setup.cost_centres_found", chargeable),
    )])
}

/// One item, with the count beside it when it is satisfied.
///
/// A satisfied line says what was found - "312 accounts" - because a tick with
/// no number beside it does not distinguish a chart from a single account
/// somebody added by hand. An unsatisfied one keeps the sentence it was
/// declared with, which says what is missing.
fn answered(item: &SetupItem, satisfied: bool, found: Message) -> SetupStatus {
    let status = SetupStatus::of(item, satisfied);

    if satisfied { status.noted(found) } else { status }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_with_declared_items_has_a_predicate_for_each() {
        // `books` and `hr` index into SETUP by position. A declaration added
        // without a predicate would answer the wrong item, or panic.
        assert_eq!(app_books::SETUP.len(), 3);
        assert_eq!(app_hr::SETUP.len(), 1);
    }

    #[test]
    fn declared_keys_are_unique_within_an_app() {
        for items in [app_books::SETUP, app_hr::SETUP] {
            for (index, item) in items.iter().enumerate() {
                assert!(
                    !items[..index].iter().any(|other| other.key == item.key),
                    "{} is declared twice",
                    item.key
                );
            }
        }
    }

    #[test]
    fn a_satisfied_item_reports_what_was_found() {
        let status = answered(&app_books::SETUP[0], true, pmsg!("books.setup.chart_found", 312));

        assert!(status.satisfied);
        assert_eq!(status.note.and_then(|note| note.count), Some(312));
    }

    #[test]
    fn an_unsatisfied_item_keeps_the_sentence_that_says_why() {
        let status = answered(&app_books::SETUP[0], false, pmsg!("books.setup.chart_found", 0));

        assert!(!status.satisfied);
        assert_eq!(
            status.note,
            Some(Message::new(app_books::SETUP[0].missing))
        );
    }
}
