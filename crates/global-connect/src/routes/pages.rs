//! The pages.
//!
//! The words are in [`crate::i18n`]; what is here is everything about a page
//! that is the same in every language - which icon an application wears, which
//! plan is drawn as the emphasised column, and the order all of them come in.
//!
//! Keeping those out of the catalogs is the point. A path on a 24-unit grid and
//! a `bool` do not translate, and a copy of them per language is a copy per
//! language to get wrong.

use askama::Template;
use axum::response::Response;
use phonix_core::i18n::Language;

use crate::i18n::{self, Strings};
use crate::state::{Links, SiteState};

/// The icon each application wears, in the order the catalogs list them.
///
/// `d` of a single `<path>`. Inline SVG rather than an icon font or a sprite:
/// the content security policy allows no external image and this crate ships no
/// image file, so an icon is markup or it is nothing.
pub const APP_ICONS: [&str; 3] = [
    // Books - a ledger, open.
    "M4 4h11l5 5v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4Zm11 0v5h5M8 13h8M8 17h5",
    // Inventory - a box seen in three dimensions.
    "M3 7.5 12 3l9 4.5v9L12 21l-9-4.5v-9Zm0 0 9 4.5m0 0 9-4.5m-9 4.5V21",
    // People.
    "M16 20v-1a4 4 0 0 0-4-4H7a4 4 0 0 0-4 4v1M9.5 7a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm11 13v-1a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8",
];

/// Which plan is drawn as the emphasised column.
pub const PLAN_FEATURED: [bool; 3] = [false, true, false];

/// Whether the numbers on the pricing page are placeholders.
///
/// One switch rather than a flag per plan: nothing has been priced, so the
/// honest thing is for the page to say so once and to stop saying it the moment
/// a real number lands. See ADR 0007 section 8.
pub const PRICES_ARE_PROVISIONAL: bool = true;

/// One entry in the language switcher.
pub struct LanguageLink {
    pub code: &'static str,
    pub native_name: &'static str,
    /// The same page in that language, which is the only href worth offering.
    /// A switcher that always lands on the home page loses somebody's place
    /// every time they use it.
    pub href: String,
    pub current: bool,
}

/// What the frame around every page needs.
///
/// A struct rather than seven fields repeated across every page's template:
/// `base.html` names `frame.product_name`, so a page that forgets one does not
/// compile, and adding an eighth is one edit rather than six.
pub struct Frame {
    pub product_name: String,
    pub links: Links,
    /// Prefixed onto every internal link, so a visitor reading in Chinese who
    /// clicks "Pricing" stays in Chinese. Empty for English.
    pub prefix: &'static str,
    pub lang: &'static str,
    pub dir: &'static str,
    pub languages: Vec<LanguageLink>,
    /// Which navigation entry to mark. A `&'static str` compared in the
    /// template rather than an enum: there are four, spelled once in
    /// `_nav.html`, and a page naming one that does not exist highlights
    /// nothing.
    pub current: &'static str,
    /// The environment, shown only when it is not production. A visitor to the
    /// real site should never see a badge; somebody looking at a staging copy
    /// should never have to wonder.
    pub badge: Option<String>,
}

impl Frame {
    /// `path` is the address of this page without a language prefix - `""` for
    /// the home page, `"/pricing"` for pricing. It is what lets the switcher
    /// offer the same page rather than the front page.
    fn new(
        state: &SiteState,
        language: Language,
        t: &'static Strings,
        current: &'static str,
        path: &str,
    ) -> Self {
        let languages = state
            .languages()
            .iter()
            .map(|offered| LanguageLink {
                code: offered.code(),
                native_name: offered.native_name(),
                href: address(*offered, path),
                current: *offered == language,
            })
            .collect();

        Self {
            product_name: state.product_name().to_owned(),
            links: state.links().clone(),
            prefix: i18n::prefix_of(language),
            // From the catalog that rendered, not from the address. They agree
            // today and the difference is the whole value of the attribute: a
            // lookup that fell back would otherwise serve English while telling
            // a screen reader to pronounce it as Chinese.
            lang: t.code,
            dir: language.direction().attribute(),
            languages,
            current,
            badge: (state.environment() != "production").then(|| state.environment().to_owned()),
        }
    }
}

/// A page's address in one language.
///
/// `/` rather than the empty string for the home page: an `href=""` re-requests
/// the current URL including its query, which is not the same link.
fn address(language: Language, path: &str) -> String {
    let prefix = i18n::prefix_of(language);

    if prefix.is_empty() && path.is_empty() {
        "/".to_owned()
    } else {
        format!("{prefix}{path}")
    }
}

/// Put the trial length into a sentence that has a `{days}` in it.
///
/// A `replace` rather than a format string, because the catalogs are plain
/// `&'static str` and a translator moving the number to the front of the
/// sentence must not have to think about argument order.
fn with_days(sentence: &str, days: u32) -> String {
    sentence.replace("{days}", &days.to_string())
}

// ---------------------------------------------------------------------------
// The pages
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomePage {
    pub frame: Frame,
    pub t: &'static Strings,
    pub icons: [&'static str; 3],
    /// From `desk.trial_days`, the only number in this estate that currently
    /// means anything commercially - a trial is a licence with an end date, and
    /// that field is what sets it.
    pub trial_note: String,
}

#[derive(Template)]
#[template(path = "product.html")]
pub struct ProductPage {
    pub frame: Frame,
    pub t: &'static Strings,
    pub icons: [&'static str; 3],
}

#[derive(Template)]
#[template(path = "pricing.html")]
pub struct PricingPage {
    pub frame: Frame,
    pub t: &'static Strings,
    pub featured: [bool; 3],
    pub trial_note: String,
    pub provisional: bool,
}

#[derive(Template)]
#[template(path = "about.html")]
pub struct AboutPage {
    pub frame: Frame,
    pub t: &'static Strings,
}

#[derive(Template)]
#[template(path = "contact.html")]
pub struct ContactPage {
    pub frame: Frame,
    pub t: &'static Strings,
}

/// A page that only has something to say: a 404, a 500.
#[derive(Template)]
#[template(path = "message.html")]
pub struct MessagePage {
    pub frame: Frame,
    pub t: &'static Strings,
    pub heading: String,
    pub detail: String,
}

pub async fn home(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&HomePage {
        frame: Frame::new(state, language, t, "home", ""),
        t,
        icons: APP_ICONS,
        trial_note: with_days(t.home.trial_note, state.config.desk.trial_days),
    })
}

pub async fn product(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&ProductPage {
        frame: Frame::new(state, language, t, "product", "/product"),
        t,
        icons: APP_ICONS,
    })
}

pub async fn pricing(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&PricingPage {
        frame: Frame::new(state, language, t, "pricing", "/pricing"),
        t,
        featured: PLAN_FEATURED,
        trial_note: with_days(t.pricing.trial_note, state.config.desk.trial_days),
        provisional: PRICES_ARE_PROVISIONAL,
    })
}

pub async fn about(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&AboutPage {
        frame: Frame::new(state, language, t, "about", "/about"),
        t,
    })
}

pub async fn contact(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&ContactPage {
        frame: Frame::new(state, language, t, "contact", "/contact"),
        t,
    })
}

/// The 404, in whichever language the address was in.
///
/// `""` as the path: a page that does not exist has no counterpart in another
/// language, so the switcher offers the home page rather than a second 404.
pub fn missing(state: &SiteState, language: Language) -> MessagePage {
    let t = i18n::strings(language);

    MessagePage {
        frame: Frame::new(state, language, t, "", ""),
        t,
        heading: t.not_found.heading.to_owned(),
        detail: t.not_found.detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every array here is indexed against a catalog's, so a length that drifts
    /// is a panic on a page rather than a compile error. The lengths are fixed
    /// in the types; this is the reminder that they are fixed *together*.
    #[test]
    fn the_icons_and_the_flags_match_the_catalogs() {
        let english = i18n::strings(Language::ENGLISH);

        assert_eq!(APP_ICONS.len(), english.apps.len());
        assert_eq!(PLAN_FEATURED.len(), english.plans.len());
    }

    /// An `href=""` re-requests the current URL, query string and all, which is
    /// not the same thing as a link to the front page.
    #[test]
    fn the_english_home_page_is_a_slash() {
        assert_eq!(address(Language::ENGLISH, ""), "/");
        assert_eq!(address(Language::ENGLISH, "/pricing"), "/pricing");
    }

    #[test]
    fn a_translated_page_keeps_its_language_in_every_link() {
        let chinese = i18n::offered()
            .into_iter()
            .find(|language| language.code() == "zh")
            .expect("Chinese is offered");

        assert_eq!(address(chinese, ""), "/zh");
        assert_eq!(address(chinese, "/pricing"), "/zh/pricing");
    }

    /// The number in the sentence, wherever the translation put it.
    #[test]
    fn the_trial_length_reaches_the_sentence() {
        let english = i18n::strings(Language::ENGLISH);

        assert!(with_days(english.home.trial_note, 30).contains("30"));
        assert!(!with_days(english.home.trial_note, 30).contains("{days}"));
    }
}
