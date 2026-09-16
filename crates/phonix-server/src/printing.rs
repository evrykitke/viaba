//! `/print/{token}` - letting a headless browser in to print one report.
//!
//! An axum handler for the same reason [`crate::auth::handoff`] is one: it is
//! reached by a plain browser navigation and answers with a cookie and a
//! redirect. The browser is ours, started by the exporter, and the token was
//! minted seconds earlier for the account that asked for the export.
//!
//! The address is the token's, never the request's. A browser that could name
//! the page it wanted would be a browser that could read any page.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use phonix_core::TenantSummary;
use phonix_web::state::AppState;

/// How long the cookie lasts. A print that takes longer than this has failed
/// for some other reason.
const COOKIE_SECS: i64 = 120;

/// Trade a print token for the page it was minted for.
pub async fn print(
    State(state): State<AppState>,
    tenant: Option<axum::Extension<TenantSummary>>,
    Path(token): Path<String>,
) -> Response {
    let Some(axum::Extension(tenant)) = tenant else {
        return (StatusCode::BAD_REQUEST, "No workspace on this address.").into_response();
    };

    let Some(address) = state.printing.address_of(&tenant.slug, &token) else {
        // Unknown, expired, withdrawn, or minted for another workspace. One
        // answer for all four: there is nothing here.
        return StatusCode::NOT_FOUND.into_response();
    };

    let cookie = phonix_web::server::cookie::set_print(
        &state.config.security.session,
        tenant.slug.as_str(),
        &token,
        COOKIE_SECS,
    );

    let Ok(cookie) = HeaderValue::from_str(&cookie) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut response = Redirect::to(&address).into_response();
    response.headers_mut().append(header::SET_COOKIE, cookie);

    response
}
