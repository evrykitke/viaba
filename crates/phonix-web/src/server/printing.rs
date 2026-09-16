//! Printing a report with a browser.
//!
//! A PDF export is the report's own page, opened in a headless browser and
//! printed - ADR 0008 §9.1. This is the half that runs the browser; the half
//! that lets it in is `/print/{token}` in `phonix-server`, and the token
//! between them is `ExportTokens` on `AppState`.
//!
//! It lives here rather than beside that route because both export paths need
//! it: the exporter in `phonix-server` for a report that grows, and a server
//! function in this crate for one bounded enough to come straight back. A
//! `phonix-web` that could not print would force the second through the queue
//! for no reason but the direction of the crates.

use std::path::Path as FilePath;
use std::time::Duration;

use phonix_core::TenantSlug;
use phonix_core::identity::UserId;

use crate::state::AppState;

/// Print one report, as the account that asked for it.
///
/// Opens the report's own page in a headless browser and takes what it prints.
/// The engine that draws the screen draws the file, so the two are one
/// document - ADR 0008 §9.1.
///
/// One process per export, killed at the configured timeout: a browser that
/// hangs on a page must cost one export rather than the worker.
pub async fn to_pdf(
    state: &AppState,
    tenant: &TenantSlug,
    user: UserId,
    address: &str,
) -> Result<Vec<u8>, String> {
    let reporting = &state.config.reporting;

    if !reporting.prints() {
        return Err("this deployment has no browser to print with".to_owned());
    }

    // A directory of its own per print: it holds the file that comes back and
    // the profile the browser writes, and both go when it does. A profile
    // shared between prints is a cookie jar shared between accounts.
    let work = std::env::temp_dir().join(format!("phonix-print-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&work).map_err(|err| format!("no room to print: {err}"))?;

    let token = state.printing.issue(
        tenant.clone(),
        user,
        address,
        Duration::from_secs(reporting.print_timeout_secs + 30),
    );

    let printed = print_with(state, &work, &token, tenant).await;

    state.printing.withdraw(&token);
    let _ = std::fs::remove_dir_all(&work);

    printed
}

/// Run the browser and read what it wrote.
async fn print_with(
    state: &AppState,
    work: &FilePath,
    token: &str,
    tenant: &TenantSlug,
) -> Result<Vec<u8>, String> {
    let reporting = &state.config.reporting;
    let file = work.join("report.pdf");
    let entry = format!(
        "{}/print/{token}",
        state.config.server.tenant_origin(tenant.as_str())
    );

    let mut browser = tokio::process::Command::new(reporting.browser.trim());

    browser
        .arg("--headless=new")
        .arg("--disable-gpu")
        // No profile of its own to find, no first-run screen, nothing to
        // report home about: this browser exists for one page.
        .arg(format!("--user-data-dir={}", work.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        // The page carries `@page` from the definition, so the paper is the
        // report's. The browser's own header and footer are not the report's
        // and would print a URL across the bottom of a customer's statement.
        .arg("--no-pdf-header-footer")
        .arg(format!("--print-to-pdf={}", file.display()))
        .arg(&entry)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    let child = browser
        .spawn()
        .map_err(|err| format!("could not start the browser: {err}"))?;

    let finished = tokio::time::timeout(
        Duration::from_secs(reporting.print_timeout_secs),
        child.wait_with_output(),
    )
    .await;

    match finished {
        Ok(Ok(output)) if output.status.success() => {}
        Ok(Ok(output)) => {
            let said = String::from_utf8_lossy(&output.stderr);
            let said = said.lines().last().unwrap_or("").trim();

            return Err(format!("the browser refused to print the report: {said}"));
        }
        Ok(Err(err)) => return Err(format!("the browser could not be run: {err}")),
        // The child is killed on drop, so the timeout takes the process with
        // it rather than leaving one behind per stuck export.
        Err(_) => {
            return Err(format!(
                "the report took longer than {} seconds to print",
                reporting.print_timeout_secs,
            ));
        }
    }

    std::fs::read(&file).map_err(|err| format!("the browser printed nothing: {err}"))
}
