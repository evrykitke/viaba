//! The half of the attachments section that only exists in a browser.
//!
//! Compiled into the wasm bundle and nowhere else. Everything here touches the
//! DOM or the network, so the server build gets the no-op stub in the parent
//! module instead.
//!
//! # Nothing in this file may panic
//!
//! `wasm32-unknown-unknown` aborts rather than unwinds, so one panic stops
//! every handler, effect and pending request in the tab at once. Every JS call
//! below is destructured with `let ... else` or matched, and a failure becomes
//! a sentence on the screen rather than a `?` that discards the reason.

use leptos::prelude::*;
use phonix_core::files::attachment::{AttachmentInput, BUCKET, RecordRef};
use phonix_core::form::Submission;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::server_fns::file_fns::{attach_file, upload_status, upload_url};

const POLL_INTERVAL_MS: i32 = 400;

/// Twenty-five polls at 400ms is ten seconds. A bound on a loop rather than a
/// timeout anybody should reach - an attachment is read, hashed and moved in
/// well under that.
const MAX_POLLS: u32 = 25;

/// A file has been chosen: send it, wait for the verdict, file it on the record.
pub(super) fn upload(
    ev: leptos::ev::Event,
    record: RecordRef,
    busy: RwSignal<bool>,
    message: RwSignal<Option<String>>,
    reload: Callback<()>,
) {
    let Some(input) = ev
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return;
    };

    let Some(file) = input.files().and_then(|files| files.get(0)) else {
        return;
    };

    // So choosing the same file twice in a row fires the event the second time.
    input.set_value("");

    busy.set(true);
    message.set(None);

    spawn_local(async move {
        let outcome = attempt(record, &file).await;
        busy.set(false);

        match outcome {
            Ok(()) => {
                let _ = reload.try_run(());
            }
            Err(reason) => message.set(Some(reason)),
        }
    });
}

/// The whole upload, with every step's failure named.
async fn attempt(record: RecordRef, file: &web_sys::File) -> Result<(), String> {
    let received = post(file).await?;

    // The upload answered "received", not "accepted": deciding what the bytes
    // are is a job that runs after the request returned. Nothing is linked to
    // the record until that job has an answer.
    let settled = poll(received).await?;

    if let Some(rejection) = settled.rejection {
        return Err(rejection.message());
    }

    if !settled.status.is_available() {
        return Err("That file could not be stored. Please try again.".to_owned());
    }

    // No caption: the file's own name is the caption, which is right often
    // enough to be worth not asking about. Renaming one is a separate act.
    let input = AttachmentInput {
        record,
        file_id: settled.id,
        title: String::new(),
    };

    match attach_file(input).await {
        Ok(Submission::Saved(_)) => Ok(()),
        Ok(Submission::Rejected(errors)) => Err(errors
            .first()
            .map(|error| crate::i18n::t(&error.message))
            .unwrap_or_else(|| "That file could not be attached.".to_owned())),
        Err(err) => Err(err.to_string()),
    }
}

/// POST the file and read back the row it created.
async fn post(file: &web_sys::File) -> Result<Uuid, String> {
    let window = web_sys::window().ok_or_else(|| "No browser window.".to_owned())?;

    let form = web_sys::FormData::new().map_err(describe)?;
    form.append_with_blob_and_filename("file", file, &file.name())
        .map_err(describe)?;

    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&form);

    let request =
        web_sys::Request::new_with_str_and_init(&upload_url(BUCKET), &init).map_err(describe)?;

    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(describe)?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "The upload gave an answer this page could not read.".to_owned())?;

    let body = JsFuture::from(response.text().map_err(describe)?)
        .await
        .map_err(describe)?
        .as_string()
        .unwrap_or_default();

    if !response.ok() {
        // The route answers `{"error": "..."}` on every refusal, and that
        // sentence is written to be shown.
        return Err(error_from(&body)
            .unwrap_or_else(|| format!("The upload was refused ({}).", response.status())));
    }

    serde_json::from_str::<phonix_core::files::FileSummary>(&body)
        .map(|summary| summary.id)
        .map_err(|_| "The upload gave an answer this page could not read.".to_owned())
}

fn error_from(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(str::to_owned)
}

/// Ask about the upload until the job has decided.
async fn poll(id: Uuid) -> Result<phonix_core::files::FileSummary, String> {
    for _ in 0..MAX_POLLS {
        match upload_status(id).await {
            Ok(Some(summary)) if summary.status.is_terminal() => return Ok(summary),
            Ok(_) => sleep(POLL_INTERVAL_MS).await,
            Err(err) => return Err(err.to_string()),
        }
    }

    Err("That file is taking longer than expected to check. It may appear shortly.".to_owned())
}

async fn sleep(milliseconds: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds);
        }
    });

    let _ = JsFuture::from(promise).await;
}

/// A JS exception as a sentence.
fn describe(err: JsValue) -> String {
    err.as_string()
        .or_else(|| {
            err.dyn_ref::<js_sys::Error>()
                .map(|error| error.message().into())
        })
        .unwrap_or_else(|| "The browser refused that.".to_owned())
}
