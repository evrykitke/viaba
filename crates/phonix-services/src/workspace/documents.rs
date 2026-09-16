//! Reading and changing what each kind of document looks like.
//!
//! The split is [`load`] and [`current`], the same one
//! [`profile`](super::profile) makes: the administration screen's read is
//! gated on `Settings`, and the read a document does while it is being drawn
//! is not - a report that only drew its letterhead for administrators would be
//! a different document depending on who printed it.

use phonix_core::permissions;
use phonix_core::report::DocumentSettings;
use phonix_db::document_settings as store;
use phonix_db::sqlx::PgPool;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// What this workspace keeps for every document type it has settings for.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<DocumentSettings>> {
    caller.require(permissions::SETTINGS)?;

    Ok(store::list(pool).await?)
}

/// One document type, as the settings screen reads it.
pub async fn load(
    pool: &PgPool,
    caller: &Caller,
    document_type: &str,
) -> ServiceResult<DocumentSettings> {
    caller.require(permissions::SETTINGS)?;

    Ok(store::load(pool, document_type).await?)
}

/// One document type, as a report reads it while drawing.
///
/// Ungated, like [`profile::current`](super::profile::current), and for the
/// same reason: it decides what a document looks like, not what anybody may
/// see. A type nobody has kept a setting for comes back as the defaults.
pub async fn current(pool: &PgPool, document_type: &str) -> ServiceResult<DocumentSettings> {
    Ok(store::load(pool, document_type).await?)
}

/// Store what an administrator chose.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    settings: &DocumentSettings,
) -> ServiceResult<()> {
    caller.require(permissions::SETTINGS)?;
    let changed_by = acting_user(caller)?;

    let errors = settings.validate();
    if !errors.is_empty() {
        return Err(ServiceError::Rejected(errors));
    }

    let previous = store::load(pool, &settings.document_type).await?;
    store::save(pool, settings, Some(changed_by)).await?;

    // On the document type's own record rather than on a settings singleton:
    // the question asked after an invoice goes out wrong is about the invoice.
    audit::updated(
        pool,
        caller,
        Target::new(kinds::DOCUMENT_SETTINGS, &settings.document_type)
            .named(&settings.document_type),
        &previous,
        settings,
    )
    .await;

    Ok(())
}
