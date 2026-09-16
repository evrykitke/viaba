//! Asking for a report to be written out, and asking after one.
//!
//! # Two paths, one menu
//!
//! A report that grows with its data raises a request and a worker writes it;
//! one bounded by the record it is about renders in this request and comes
//! back at once. Which path a report takes is its definition's declaration -
//! see `Extent` - and never a guess made at the call site.
//!
//! # The permission is the report's
//!
//! Both paths resolve it from [`reports::server_report`] rather than from what
//! arrived. A request naming its own permission would be a caller choosing the
//! gate it is let through.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use phonix_core::report::{ExportFormat, ExportRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value as Parameters;
use uuid::Uuid;

/// A report written in this request, for one bounded enough to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenNow {
    /// What a browser saves it as.
    pub file_name: String,
    /// The file itself. Bytes rather than text, because a PDF is not text and
    /// a format that came back as a string would be corrupted on the way.
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// Raise an export for a report that grows with its data.
#[server(name = RaiseExport, prefix = "/api", endpoint = "reports/export", input = Json)]
pub async fn raise_export(
    report_id: String,
    parameters: Parameters,
    format: ExportFormat,
) -> Result<ExportRequest, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let report = crate::reports::server_report(&report_id)
        .ok_or_else(|| ServerFnError::new("There is no such report."))?;

    let asked = phonix_core::report::NewExport {
        report_id,
        parameters,
        format,
    };

    let raised = phonix_services::report::exports::raise(&pool, &caller, &asked, report.permission)
        .await
        .map_err(service_error)?;

    // After the row, never before: the news is a shortcut and the row is the
    // work. A worker told about a request the database had not accepted would
    // be rendering something nobody asked for.
    if let Ok(state) = crate::state::app_state()
        && let Some(exports) = &state.exports
        && let Ok(tenant) = crate::state::tenant_from_request().await
    {
        let _ = exports.0.send((tenant.slug, raised.id));
    }

    Ok(raised)
}

/// How far an export has got. The requester's alone to ask.
#[server(name = ExportState, prefix = "/api", endpoint = "reports/export/state")]
pub async fn export_state(id: Uuid) -> Result<ExportRequest, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::report::exports::load(&pool, &caller, id)
        .await
        .map_err(service_error)
}

/// Write a bounded report out now and hand back the bytes.
///
/// No row, no worker, no file: a receipt is one payment and its allocations,
/// and a round trip through a queue to produce one page is a slower answer and
/// a second set of failures for nothing.
///
/// Drawn on the server even though the browser has the report on screen,
/// because the writers live in `phonix-services` and the browser's half of
/// this crate does not link it.
#[server(name = WriteReportNow, prefix = "/api", endpoint = "reports/write", input = Json)]
pub async fn write_now(
    report_id: String,
    parameters: Parameters,
    format: ExportFormat,
) -> Result<WrittenNow, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let report = crate::reports::server_report(&report_id)
        .ok_or_else(|| ServerFnError::new("There is no such report."))?;

    caller.require(report.permission).map_err(service_error)?;

    let bytes = match format {
        // A page, so the browser that draws the screen draws the file.
        ExportFormat::Pdf => {
            let state = crate::state::app_state()?;
            let tenant = crate::state::tenant_from_request()
                .await
                .map_err(ServerFnError::new)?;
            let address = crate::reports::address(&report_id, &parameters)
                .ok_or_else(|| ServerFnError::new("That report has no address to print."))?;

            let user = caller
                .user_id()
                .ok_or_else(|| ServerFnError::new("Only an account can print a report."))?;

            crate::server::printing::to_pdf(&state, &tenant.slug, user, &address)
                .await
                .map_err(ServerFnError::new)?
        }
        ExportFormat::Csv => {
            let rendered = crate::reports::render(&pool, &caller, &report_id, &parameters)
                .await
                .map_err(service_error)?;

            phonix_services::report::writers::to_csv(&rendered).into_bytes()
        }
        ExportFormat::Xlsx => {
            let rendered = crate::reports::render(&pool, &caller, &report_id, &parameters)
                .await
                .map_err(service_error)?;

            phonix_services::report::spreadsheet::to_xlsx(&rendered).map_err(ServerFnError::new)?
        }
    };

    Ok(WrittenNow {
        file_name: format!("{report_id}.{}", format.as_str()),
        bytes,
        content_type: format.content_type().to_owned(),
    })
}
