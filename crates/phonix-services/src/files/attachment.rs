//! Attaching a file to a record, and taking it off again.
//!
//! # The permission is the record's, not the file's
//!
//! This is the whole of the difficulty. An attachment is addressed by an
//! entity kind and an id, and nothing in that pair says who may see it - so a
//! service that gated on `Pages.Files.Upload` alone would let anybody who may
//! upload *anything* list the filenames on a bill they are not allowed to
//! open. "Supplier invoice - ACME - 40,000" is most of the document.
//!
//! So every call resolves the record's own permission through [`gate`] and
//! requires that. Attaching and detaching additionally require
//! `Pages.Files.Upload`, because they add and remove files in a workspace.
//!
//! An entity this table does not name is **refused**, not waved through. The
//! cost of that is that attaching to a new kind of record needs a line here;
//! the cost of the other default is a hole that opens by omission.
//!
//! # Detaching is not deleting
//!
//! Removing an attachment removes the link. The file stays in the workspace,
//! still listed on the files screen, still deletable there by somebody with
//! `Pages.Files.Delete` - which is the permission that decides whether bytes
//! go, and it is deliberately not this one.

use phonix_core::audit::kinds;
use phonix_core::files::UploadStatus;
use phonix_core::files::attachment::{
    Attachment, AttachmentError, AttachmentInput, MAX_PER_RECORD, RecordRef,
};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::PgPool;
use phonix_db::attachment as store;
use phonix_db::files as files_db;
use phonix_db::sqlx::PgExecutor;
use uuid::Uuid;

use crate::caller::Caller;
use crate::error::{ServiceError, ServiceResult};

/// What somebody must hold to see a record of this kind at all.
///
/// The read permission rather than the write one: seeing the paperwork behind
/// a receipt is seeing the receipt, and nothing more.
fn gate(entity_type: &str) -> Option<&'static str> {
    let permission = match entity_type {
        _ if entity_type == kinds::RECEIPT.name => permissions::RECEIPTS,
        _ if entity_type == kinds::PURCHASE_ORDER.name => permissions::PURCHASE_ORDERS,
        _ if entity_type == kinds::BILL.name => permissions::BILLS,
        _ if entity_type == kinds::REQUISITION.name => permissions::REQUISITIONS,
        _ if entity_type == kinds::CONSOLIDATION.name => permissions::CONSOLIDATIONS,
        _ if entity_type == kinds::STOCK_TRANSFER.name => permissions::TRANSFERS,
        _ if entity_type == kinds::LANDED_COST.name => permissions::LANDED_COSTS,
        _ if entity_type == kinds::SALES_INVOICE.name => permissions::INVOICES,
        _ if entity_type == kinds::EMPLOYEE.name => permissions::EMPLOYEES,
        _ if entity_type == kinds::ITEM.name => permissions::ITEMS,
        _ if entity_type == kinds::PARTY.name => permissions::PARTIES,
        _ => return None,
    };

    Some(permission)
}

fn allow(caller: &Caller, record: &RecordRef) -> ServiceResult<()> {
    let Some(permission) = gate(&record.entity_type) else {
        return Err(ServiceError::rejected(
            "record",
            msg!("attachments.error.no_record"),
        ));
    };

    caller.require(permission)
}

/// Everything attached to one record.
pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    record: &RecordRef,
) -> ServiceResult<Vec<Attachment>> {
    allow(caller, record)?;
    Ok(store::for_record(pool, record).await?)
}

/// Link an already-uploaded file to a record.
///
/// The bytes went to `/files/upload?bucket=attachments` before this was
/// called; all that happens here is the link and the checks around it.
pub async fn attach(
    pool: &PgPool,
    caller: &Caller,
    input: AttachmentInput,
) -> ServiceResult<Submission<Attachment>> {
    allow(caller, &input.record)?;
    caller.require(permissions::FILES_UPLOAD)?;

    let checked = match input.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // Only a file that finished being checked. A row still in `received` has
    // bytes in quarantine, and a link to it would be a row offering a download
    // that cannot be served.
    let Some(file) = files_db::load(pool, checked.file_id).await? else {
        return Ok(reject(AttachmentError::FileNotStored));
    };
    if file.status != UploadStatus::Stored {
        return Ok(reject(AttachmentError::FileNotStored));
    }

    if store::count_for(pool, &checked.record).await? >= MAX_PER_RECORD as i64 {
        return Ok(reject(AttachmentError::TooMany));
    }

    let stored = store::insert(
        pool,
        &checked.record,
        checked.file_id,
        checked.stored_title(),
        caller.user_id(),
    )
    .await?;

    if stored.is_none() {
        return Ok(reject(AttachmentError::AlreadyAttached));
    }

    // Read back rather than assembled here: the row a screen draws carries the
    // uploader's name and the file's detected type, and neither is in hand.
    let attached = store::for_record(pool, &checked.record).await?;

    attached
        .into_iter()
        .find(|row| row.file.id == checked.file_id)
        .map(Submission::Saved)
        .ok_or_else(|| ServiceError::rejected("file_id", msg!("attachments.gone")))
}

/// Take one off. The file itself stays in the workspace.
pub async fn detach(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    let Some((record, _)) = store::find(pool, id).await? else {
        return Ok(Submission::rejected("id", msg!("attachments.gone")));
    };

    allow(caller, &record)?;
    caller.require(permissions::FILES_UPLOAD)?;

    if !store::delete(pool, id).await? {
        return Ok(Submission::rejected("id", msg!("attachments.gone")));
    }

    Ok(Submission::Saved(()))
}

/// Unlink everything on a record, as part of deleting it.
///
/// Called inside the transaction that removes the record - see the header of
/// migration 0022. No permission check: the caller has already established
/// that this record may be deleted, which is a stronger right than this one.
pub async fn detach_all<'e, E>(executor: E, record: &RecordRef) -> ServiceResult<u64>
where
    E: PgExecutor<'e>,
{
    Ok(store::delete_for_record(executor, record).await?)
}

fn reject<T>(err: AttachmentError) -> Submission<T> {
    Submission::rejected(err.field(), err.message())
}
