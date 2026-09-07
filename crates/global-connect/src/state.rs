//! What every handler needs, which is very little.
//!
//! No pool, no cache, no broker, no crypto - see ADR 0007 section 2. What is
//! here is the configuration, the addresses derived from it once at startup,
//! and the rate limiter's counters.

use std::sync::Arc;

use phonix_config::AppConfig;
use phonix_core::i18n::Language;

/// The addresses every page links to.
///
/// Resolved once rather than per request: they are built by string formatting
/// from `[server]`, they cannot change while the process runs, and every page
/// renders at least two of them.
#[derive(Clone)]
pub struct Links {
    /// Where "Get started" goes.
    pub signup: String,
    /// Where "Sign in" goes.
    pub sign_in: String,
    /// The `mailto:` behind "Talk to us", or `None` to render no link.
    pub contact: Option<String>,
    /// Absolute URLs from `[app.links]`, which the product's signed-out screens
    /// already use. Empty renders nothing, for the reason written there.
    pub privacy: Option<String>,
    pub terms: Option<String>,
}

#[derive(Clone)]
pub struct SiteState {
    pub config: Arc<AppConfig>,
    links: Links,
    /// The languages the switcher offers, resolved once.
    ///
    /// A `Vec` built at startup rather than on each request: it is derived by
    /// filtering the product's list, every page renders it, and it cannot
    /// change while the process runs.
    languages: Arc<Vec<Language>>,
    limiter: Arc<phonix_limit::Limiter>,
}

impl SiteState {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let links = Links {
            signup: config.site.signup_url(&config.server),
            sign_in: config.site.sign_in_url(&config.server),
            contact: config.site.contact_mailto(),
            privacy: config.app.links.privacy().map(str::to_owned),
            terms: config.app.links.terms().map(str::to_owned),
        };

        Self {
            config,
            links,
            languages: Arc::new(crate::i18n::offered()),
            limiter: Arc::new(phonix_limit::Limiter::new()),
        }
    }

    pub fn languages(&self) -> &[Language] {
        &self.languages
    }

    pub fn links(&self) -> &Links {
        &self.links
    }

    pub fn site(&self) -> &phonix_config::SiteConfig {
        &self.config.site
    }

    pub fn product_name(&self) -> &str {
        &self.config.site.product_name
    }

    pub fn environment(&self) -> &str {
        &self.config.app.environment
    }

    pub fn limiter(&self) -> &phonix_limit::Limiter {
        &self.limiter
    }
}
