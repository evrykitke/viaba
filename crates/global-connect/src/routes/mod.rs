//! Everything the site answers, and the headers it answers with.
//!
//! Every route here is a `GET`. There is no `POST` anywhere in this crate and
//! that is a decision rather than a stage not reached yet - ADR 0007 section 5.
//! A form needs somewhere to put what it receives, which is SMTP or the
//! catalog, and either one hands back the dependency the site exists without.
//!
//! # A language is an address, not a preference
//!
//! `/pricing` is English and `/zh/pricing` is Chinese. Not a cookie, not a
//! `Vary: Accept-Language`, and no redirect off the front page.
//!
//! Three reasons, in the order they mattered. A page that varies by header is a
//! page a shared cache holds one arbitrary version of, and the pages here are
//! cacheable for five minutes precisely because they hold nothing about
//! anybody. A search engine needs one address per language to index, which is
//! what the `hreflang` links in `base.html` declare. And this site sets no
//! cookie at all - see ADR 0007 section 13 - so there is nowhere to keep a
//! preference even if one were wanted.
//!
//! The consequence is that a visitor arrives in English and switches, rather
//! than arriving in their own language. That is the trade, and it is the one
//! every site this one is modelled on makes.

pub mod pages;

use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Router, middleware};
use http::{StatusCode, Uri, header};
use phonix_core::i18n::Language;

use crate::i18n;
use crate::state::SiteState;

/// Everything the site serves.
///
/// The five pages are registered once per offered language, from one list, so a
/// page that exists in English cannot quietly not exist in Chinese.
pub fn router(state: SiteState) -> Router {
    let mut router = Router::new();

    for language in i18n::offered() {
        let prefix = i18n::prefix_of(language);
        let root = if prefix.is_empty() { "/" } else { prefix };

        router = router
            .route(
                root,
                get(move |State(state): State<SiteState>| async move {
                    pages::home(&state, language).await
                }),
            )
            .route(
                &format!("{prefix}/solutions"),
                get(move |State(state): State<SiteState>| async move {
                    pages::solutions(&state, language).await
                }),
            )
            .route(
                &format!("{prefix}/product"),
                get(move |State(state): State<SiteState>| async move {
                    pages::product(&state, language).await
                }),
            )
            .route(
                &format!("{prefix}/pricing"),
                get(move |State(state): State<SiteState>| async move {
                    pages::pricing(&state, language).await
                }),
            )
            .route(
                &format!("{prefix}/about"),
                get(move |State(state): State<SiteState>| async move {
                    pages::about(&state, language).await
                }),
            )
            .route(
                &format!("{prefix}/contact"),
                get(move |State(state): State<SiteState>| async move {
                    pages::contact(&state, language).await
                }),
            );
    }

    // The pictures, each at its own hashed address. A route per known file and
    // not a directory: there is no path to traverse when the only addresses
    // that exist are ones the build put here.
    for shot in crate::artifacts::SHOTS
        .iter()
        .chain(crate::artifacts::SOCIAL.as_ref())
    {
        router = router.route(shot.url, get(move || artifact(shot)));
    }

    if let Some((url, bytes)) = crate::artifacts::HANDWRITING {
        router = router.route(url, get(move || font(bytes)));
    }

    router
        // The two assets. Each path carries a content hash, which is what lets
        // them be cached forever - see `crate::assets`.
        .route(crate::assets::STYLESHEET, get(stylesheet))
        .route(crate::assets::SCRIPT, get(script))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        // For the systemd unit and nothing else.
        .route("/health", get(health))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::limit::enforce,
        ))
        .with_state(state)
}

/// Liveness, in the shape the other two binaries already use.
///
/// No dependency is touched, because there are none to touch.
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// The opposite of Desk's rule.
///
/// Desk is `noindex, nofollow` on every page because it suspends workspaces.
/// This is the one surface in the estate that *wants* to be found, so the only
/// thing kept out of an index is the health probe.
///
/// The `Sitemap:` line is absolute because the format requires it, which is one
/// more reason `site.public_url` is refused blank under production.
async fn robots(State(state): State<SiteState>) -> impl IntoResponse {
    let body = format!(
        "User-agent: *\nDisallow: /health\n\nSitemap: {}/sitemap.xml\n",
        state.origin()
    );

    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
}

/// Every page, in every language, with the alternates spelled out.
///
/// Written by hand rather than through an XML crate: it is six paths by two
/// languages and the shape never varies, so a dependency would be carrying a
/// parser to emit a document with no user input in it. The only values
/// interpolated are an origin from configuration and paths from [`pages::PATHS`]
/// - both `&'static str` or already validated - so there is nothing here that
/// could need escaping.
///
/// `xhtml:link` alternates on every entry, including the entry's own language,
/// which is what the specification asks for and what a validator complains
/// about when it is missing.
async fn sitemap(State(state): State<SiteState>) -> impl IntoResponse {
    let origin = state.origin();
    let languages = i18n::offered();

    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" \
         xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );

    for path in pages::PATHS {
        for language in &languages {
            body.push_str("  <url>\n");
            body.push_str(&format!(
                "    <loc>{origin}{}</loc>\n",
                pages::address(*language, path)
            ));

            for alternate in &languages {
                body.push_str(&format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{origin}{}\"/>\n",
                    alternate.code(),
                    pages::address(*alternate, path)
                ));
            }

            body.push_str("  </url>\n");
        }
    }

    body.push_str("</urlset>\n");

    (
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
}

/// A screenshot, or the social card.
///
/// `immutable`, like the stylesheet and for the same reason: the name carries a
/// hash of the bytes, so a changed picture is a different address and there is
/// nothing here a browser could hold that is wrong.
async fn artifact(shot: &'static crate::artifacts::Shot) -> Response {
    (
        [
            (header::CONTENT_TYPE, shot.mime),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        shot.bytes,
    )
        .into_response()
}

/// The handwriting face, when one has been dropped in.
///
/// `font-src 'self'` in the policy already allows exactly this and nothing
/// else, so a face fetched from a font CDN would fail visibly rather than
/// quietly becoming a third-party request on every page.
async fn font(bytes: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response()
}

/// The 404, in the language the address was written in.
///
/// `/zh/nonsense` answers in Chinese. Somebody who is reading the site in one
/// language and mistypes has not thereby asked for another one.
pub async fn not_found(State(state): State<SiteState>, uri: Uri) -> Response {
    let page = pages::missing(&state, language_of(uri.path()));

    (StatusCode::NOT_FOUND, render(&page)).into_response()
}

/// Which language a path is written in, from its first segment.
fn language_of(path: &str) -> Language {
    path.split('/')
        .nth(1)
        .and_then(i18n::from_prefix)
        .unwrap_or(Language::ENGLISH)
}

/// The stylesheet.
///
/// Compiled into the binary rather than read from a directory beside it, so
/// the site stays one artefact: copy the binary, run it. `immutable` is safe to
/// the point of being the reason the name has a hash in it - a changed
/// stylesheet is a different address, so there is nothing here a browser could
/// hold that is wrong.
async fn stylesheet() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        crate::assets::STYLESHEET_CSS,
    )
        .into_response()
}

/// The one script, served the same way and cached the same way.
///
/// Everything it does is an enhancement - see `script/site.js`. A separate file
/// rather than a `<script>` block so the content security policy can stay
/// `script-src 'self'` with no `unsafe-inline`.
async fn script() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        crate::assets::SCRIPT_JS,
    )
        .into_response()
}

/// An HTML response with the headers every page carries.
///
/// `public, max-age=300` and not Desk's `no-store`: these pages hold no
/// session, no workspace name and nothing anybody typed, so a proxy holding one
/// for five minutes is the cheapest thing that can happen to this site. Five
/// minutes rather than a day because the pages are edited by hand and a stale
/// price is worse than a slow one.
///
/// No `Vary: Accept-Language`, because nothing here reads that header - the
/// language is in the address. See this module's header for why.
pub fn html_response(body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            // `strict-origin-when-cross-origin` and not Desk's `same-origin`:
            // the whole job of this site is sending people to another host, and
            // the product's own logs should be able to see that the referrer
            // was here.
            (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
            (
                header::CONTENT_SECURITY_POLICY,
                // One stylesheet and one script, both from this binary, and
                // nothing from anywhere else. `img-src 'self' data:` because
                // the artwork is inline SVG and the favicon is a data URI -
                // there is no image file in this crate and no CDN behind one.
                //
                // `'self'` and never `'unsafe-inline'`: the only code that can
                // run on a page here is code this binary served.
                "default-src 'none'; style-src 'self'; script-src 'self'; \
                 img-src 'self' data:; font-src 'self'; form-action 'none'; \
                 base-uri 'none'; frame-ancestors 'none'",
            ),
        ],
        body,
    )
        .into_response()
}

/// Render a template into a response, or report the failure.
///
/// A render can only fail on a formatter error, which in practice means an out
/// of memory condition - but it returns a `Result`, and the alternative to
/// handling it is an `unwrap` in the one place a panic takes the page down.
pub fn render(template: &impl Template) -> Response {
    match template.render() {
        Ok(body) => html_response(body),
        Err(err) => {
            tracing::error!(error = %err, "failed to render a page");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "Something went wrong rendering this page.",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mistyped_address_answers_in_the_language_it_was_written_in() {
        assert_eq!(language_of("/zh/nonsense").code(), "zh");
        assert_eq!(language_of("/nonsense").code(), "en");
        assert_eq!(language_of("/"), Language::ENGLISH);
    }

    /// `/fr/...` is a language the product speaks and the site has not been
    /// written in. It must answer an English 404 rather than pretend.
    #[test]
    fn an_untranslated_prefix_is_not_a_language() {
        assert_eq!(language_of("/fr/pricing"), Language::ENGLISH);
    }
}
