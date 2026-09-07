//! The site's own words.
//!
//! English and Chinese today, and the machinery for the rest. See ADR 0007
//! section 8: the plumbing was built now because retrofitting a language into
//! finished templates is far more expensive than carrying it from the start,
//! and Chinese was written second because a proof needs a language that is not
//! a near-neighbour of the original - if the layout survives Chinese it will
//! survive German.
//!
//! # Why this is not `phonix_core::i18n`
//!
//! The product's catalog is a `HashMap<String, String>` loaded from
//! `locales/<code>.json` at boot, with `msg!` const-asserting at compile time
//! that a key exists. That is the right shape for an application: thousands of
//! short keys, edited by translators, overridable per deployment.
//!
//! This is prose. Ninety-odd sentences, edited by whoever is writing the
//! marketing, and a missing one is a blank headline rather than a fallback
//! nobody notices. So the strings are a `struct` and each language is a `static`
//! of it: **a language that has forgotten a sentence does not compile.** No
//! parity test, no build script, no file read at request time - the same
//! property the product buys with `build.rs`, bought here with the type system
//! because the shape is small enough to afford it.
//!
//! The cost is real and worth writing down: every edit to a sentence is an edit
//! per language. That is the standing bill for a translated front door, and it
//! is why [`has_catalog`] lists what has actually been written rather than what
//! the product speaks.
//!
//! # The language list is not ours
//!
//! [`phonix_core::i18n::Language`] owns it - the codes, the native names, the
//! direction. This crate names a code in exactly one place, [`has_catalog`],
//! and takes everything else from there.

mod en;
mod zh;

use phonix_core::i18n::Language;

/// Every string on the site, for one language.
///
/// Grouped by page rather than flat. A flat struct of ninety fields is a wall
/// nobody can review; grouped, a translator opening `zh.rs` can see which page
/// they are in without counting.
pub struct Strings {
    /// Which language this catalog is written in, as its code.
    ///
    /// A code rather than a [`Language`]: naming the language would mean each
    /// catalog reaching into `Language::ALL` for it, and the honest way to do
    /// that in a `static` is an index - which compiles and is wrong the moment
    /// somebody inserts a language above it. A code is compared against the one
    /// it was looked up by, so the same mistake is a failing test instead.
    pub code: &'static str,
    pub common: Common,
    pub nav: Nav,
    pub footer: Footer,
    pub home: Home,
    pub product: Product,
    pub pricing: Pricing,
    pub about: About,
    pub contact: Contact,
    pub not_found: NotFound,
    /// Fixed-length, so a language that describes two applications where
    /// English describes three does not compile. The same for the pillars and
    /// the plans: a translation is complete or it is a build failure.
    pub apps: [AppCopy; 3],
    pub pillars: [PillarCopy; 4],
    pub plans: [PlanCopy; 3],
}

pub struct Common {
    pub start_free: &'static str,
    pub get_started: &'static str,
    pub sign_in: &'static str,
    pub talk_to_us: &'static str,
    pub see_inside: &'static str,
    pub tagline: &'static str,
    pub skip_to_content: &'static str,
    pub menu: &'static str,
    /// Names the language switcher for a screen reader.
    pub language: &'static str,
    pub home_of: &'static str,
}

pub struct Nav {
    pub product: &'static str,
    pub pricing: &'static str,
    pub about: &'static str,
    pub contact: &'static str,
    pub main: &'static str,
}

pub struct Footer {
    pub product: &'static str,
    pub company: &'static str,
    pub account: &'static str,
    pub whats_inside: &'static str,
    pub privacy: &'static str,
    pub terms: &'static str,
    pub rights: &'static str,
}

pub struct Home {
    pub title: &'static str,
    pub description: &'static str,
    pub eyebrow: &'static str,
    /// The headline, in two halves: the second is drawn in the accent gradient,
    /// so it is two fields rather than one with markup in it. A translation
    /// that wants the emphasis in a different place moves the split.
    pub headline_lead: &'static str,
    pub headline_accent: &'static str,
    pub lede: &'static str,
    /// `{days}` is replaced with `desk.trial_days`.
    pub trial_note: &'static str,
    pub apps_title: &'static str,
    pub apps_lede: &'static str,
    pub apps_more: &'static str,
    pub dense_title: &'static str,
    pub dense_lede: &'static str,
    pub reasons_title: &'static str,
    pub cta_title: &'static str,
    pub cta_body: &'static str,
}

pub struct Product {
    pub title: &'static str,
    pub description: &'static str,
    pub eyebrow: &'static str,
    pub headline: &'static str,
    pub lede: &'static str,
    pub underneath: &'static str,
    pub beneath: [Beneath; 4],
    pub cta_title: &'static str,
    pub cta_body: &'static str,
}

pub struct Beneath {
    pub heading: &'static str,
    pub body: &'static str,
}

pub struct Pricing {
    pub title: &'static str,
    pub description: &'static str,
    pub eyebrow: &'static str,
    pub headline: &'static str,
    pub lede: &'static str,
    pub most: &'static str,
    /// `{days}` is replaced with `desk.trial_days`.
    pub trial_note: &'static str,
    pub provisional_lead: &'static str,
    pub provisional_body: &'static str,
    pub faq_title: &'static str,
    pub faq: [Beneath; 4],
}

pub struct About {
    pub title: &'static str,
    pub description: &'static str,
    pub eyebrow: &'static str,
    pub headline: &'static str,
    pub body: [&'static str; 3],
    pub values_title: &'static str,
    pub cta_title: &'static str,
    pub cta_body: &'static str,
}

pub struct Contact {
    pub title: &'static str,
    pub description: &'static str,
    pub eyebrow: &'static str,
    pub headline: &'static str,
    pub lede: &'static str,
    pub cards: [Beneath; 3],
}

pub struct NotFound {
    pub title: &'static str,
    pub heading: &'static str,
    pub detail: &'static str,
}

/// One application, in one language.
///
/// The icon and the order are *not* here: a path on a 24-unit grid is the same
/// in every language, and a copy per catalog is a copy per catalog to get
/// wrong. See [`crate::routes::pages::APP_ICONS`].
pub struct AppCopy {
    pub name: &'static str,
    pub tagline: &'static str,
    pub points: &'static [&'static str],
    /// `None` means the application is finished.
    pub status: Option<&'static str>,
}

pub struct PillarCopy {
    pub heading: &'static str,
    pub body: &'static str,
}

pub struct PlanCopy {
    pub name: &'static str,
    pub who: &'static str,
    pub price: &'static str,
    pub cadence: &'static str,
    pub points: &'static [&'static str],
    pub action: &'static str,
}

/// Whether the site has prose in this language.
///
/// The one place a language code is written down in this crate, and the reason
/// it is a single function: the switcher, the `hreflang` links, the URL prefixes
/// and the catalog lookup all read it, and four lists of two codes is three
/// chances to disagree.
///
/// The site's set is a *subset* of the product's. `Language::ALL` is what the
/// application speaks - English, German, French and Chinese, all four at exact
/// catalog parity - and this is what the front door has been written in.
/// Advertising a language and then serving English is worse than not
/// advertising it.
///
/// Adding French is a `fr.rs` beside `en.rs`, an arm in [`strings`], and `"fr"`
/// here.
pub const fn has_catalog(language: Language) -> bool {
    matches!(language.code().as_bytes(), b"en" | b"zh")
}

/// The languages the switcher offers, in the product's own order.
///
/// Resolved once at startup and carried on the state rather than rebuilt per
/// request - see [`crate::state::SiteState`]. Derived from `Language::ALL`
/// rather than listed again, so the native names and the direction are the
/// product's and cannot drift.
pub fn offered() -> Vec<Language> {
    Language::ALL
        .iter()
        .copied()
        .filter(|language| has_catalog(*language))
        .collect()
}

/// The catalog for a language, falling back to English.
///
/// A `match` rather than a map: two arms the compiler checks, and no allocation
/// to look one up on a request that is going to render in microseconds.
pub fn strings(language: Language) -> &'static Strings {
    match language.code() {
        "zh" => &zh::STRINGS,
        _ => &en::STRINGS,
    }
}

/// The language a URL prefix names, or `None` if it names none.
///
/// English has no prefix: `/pricing` rather than `/en/pricing`. The default
/// language living at the root is what keeps every link somebody has already
/// shared working, and it is what every site this one is modelled on does.
pub fn from_prefix(prefix: &str) -> Option<Language> {
    Language::ALL
        .iter()
        .copied()
        .find(|language| language.code() == prefix)
        .filter(|language| has_catalog(*language) && *language != Language::ENGLISH)
}

/// The path prefix for a language, empty for English.
///
/// Used to build every link on the page, so a German visitor clicking "Pricing"
/// stays in German. A link that silently drops the language is how a translated
/// site loses somebody on their second click.
pub fn prefix_of(language: Language) -> &'static str {
    match language.code() {
        "zh" => "/zh",
        // English, and anything the site has no prose for. A wrong answer here
        // is a link that lands in English rather than a panic, which is the
        // right failure for a language there was nothing to show in anyway.
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the switcher rests on: everything it lists renders in that
    /// language. An entry in [`offered`] with no catalog behind it would
    /// advertise Chinese and serve English.
    #[test]
    fn everything_offered_has_a_catalog_of_its_own() {
        for language in offered() {
            let strings = strings(language);

            assert_eq!(
                strings.code,
                language.code(),
                "{} is offered but resolves to another language's catalog",
                language.code()
            );
        }
    }

    /// The site cannot offer a language the application does not speak: the
    /// visitor's next click is into the product.
    #[test]
    fn the_site_offers_no_language_the_product_does_not() {
        for language in offered() {
            assert!(Language::ALL.contains(&language));
        }
    }

    /// Two, today, and deliberately fewer than the product's four. This is the
    /// line that has to be edited when a translation lands, which is the point
    /// of it - see ADR 0007 section 8.
    #[test]
    fn english_and_chinese_are_what_is_written() {
        let codes: Vec<_> = offered().iter().map(|l| l.code()).collect();

        assert_eq!(codes, vec!["en", "zh"]);
    }

    #[test]
    fn english_lives_at_the_root() {
        assert_eq!(prefix_of(Language::ENGLISH), "");
        assert_eq!(from_prefix("en"), None);
    }

    #[test]
    fn a_prefix_round_trips() {
        for language in offered().iter().filter(|l| **l != Language::ENGLISH) {
            let prefix = prefix_of(*language);

            assert_eq!(prefix, format!("/{}", language.code()));
            assert_eq!(from_prefix(language.code()), Some(*language));
        }
    }

    #[test]
    fn junk_in_the_first_segment_is_not_a_language() {
        assert_eq!(from_prefix("assets"), None);
        assert_eq!(from_prefix(""), None);
        assert_eq!(from_prefix("EN"), None);
    }

    /// A language the product speaks and the site has not been written in must
    /// not answer on a prefix. `/fr/pricing` is a 404, not English wearing a
    /// French address - which is the thing search engines punish.
    #[test]
    fn an_untranslated_language_has_no_address() {
        assert_eq!(from_prefix("de"), None);
        assert_eq!(from_prefix("fr"), None);
    }
}
