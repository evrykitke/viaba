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

use crate::artifacts::{self, Shot};
use crate::i18n::{self, Strings};
use crate::state::{Links, SiteState};

/// One flat shape of an application's mark.
///
/// `fill` is a presentation attribute, not a `style` rule - which is the only
/// reason a multi-colour icon is possible here at all. The content security
/// policy is `style-src 'self'` with no `unsafe-inline`, so it forbids `style=`
/// on any element; Desk hit the same wall and reached the same answer for its
/// colour swatches.
pub struct Layer {
    pub d: &'static str,
    pub fill: &'static str,
}

/// An application's mark: flat, geometric, several colours, on a 48 grid.
///
/// The Google-catalogue idea rather than Google's icons - overlapping flat
/// shapes in two or three tones of one hue, with a second hue for the part that
/// carries the meaning. The hues are this site's own; nothing here is theirs.
///
/// Drawn at 48 rather than the product's 24 because that is what the geometry
/// needs: an isometric box built on a 24 grid lands its vertices on half
/// pixels, and a flat icon with a soft edge looks like a mistake rather than a
/// style.
///
/// `class` is how the colour reaches the tile behind the mark. It cannot be an
/// inline custom property for the reason above, so each application has a class
/// in `site.css` that sets `--app`.
pub struct Mark {
    pub layers: &'static [Layer],
    pub class: &'static str,
}

/// Which applications exist, in the order they are worth reading about.
///
/// Also the list a screenshot's filename is checked against - see
/// `artifacts/README.md`. The build script carries its own copy so a mistyped
/// filename fails at the moment somebody adds it; [`the_slugs_match_the_build`]
/// fails if the two ever disagree.
pub const APP_SLUGS: [&str; 3] = ["books", "inventory", "people"];

/// The marks, in `APP_SLUGS` order.
pub static APP_MARKS: [Mark; 3] = [
    // Books - a statement, with its corner turned and three bars on it. A
    // document rather than a coin or a ledger book: what this application
    // produces that anybody outside it ever sees is a report.
    Mark {
        class: "app-books",
        layers: &[
            Layer {
                d: "M12 4h18l10 10v28a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
                fill: "#4c6fff",
            },
            // The fold, a tone up. Two tones of one hue plus an accent is the
            // whole colour rule here.
            Layer { d: "M30 4l10 10H32a2 2 0 0 1-2-2V4z", fill: "#9db2ff" },
            Layer { d: "M17 32h5v7h-5z", fill: "#ffc24b" },
            Layer { d: "M25 26h5v13h-5z", fill: "#ffffff" },
            Layer { d: "M33 21h5v18h-5z", fill: "#3ddc97" },
        ],
    },
    // Inventory - a box in isometric, three faces in three tones, with a strip
    // of tape across the top. The one icon that has to say "a physical thing in
    // a place" rather than "a number".
    Mark {
        class: "app-inventory",
        layers: &[
            Layer { d: "M24 5l18 9-18 9-18-9z", fill: "#5ee6c5" },
            Layer { d: "M6 14v20l18 9V23z", fill: "#128f77" },
            Layer { d: "M42 14v20l-18 9V23z", fill: "#2bc4a4" },
            Layer { d: "M24 9.5l9 4.5-9 4.5-9-4.5z", fill: "#ffc24b" },
        ],
    },
    // People - two figures, the front one overlapping the back. The overlap is
    // the point: a department is people, not headcount.
    Mark {
        class: "app-people",
        layers: &[
            Layer {
                d: "M26.5,16 a5.5,5.5 0 1,0 11,0 a5.5,5.5 0 1,0 -11,0z",
                fill: "#b79cff",
            },
            Layer { d: "M32 24c5.5 0 10 4.5 10 10v7H22v-7c0-5.5 4.5-10 10-10z", fill: "#b79cff" },
            Layer { d: "M12,19 a7,7 0 1,0 14,0 a7,7 0 1,0 -14,0z", fill: "#ff5d73" },
            Layer { d: "M19 28c7.2 0 13 5.8 13 13v3H6v-3c0-7.2 5.8-13 13-13z", fill: "#ff5d73" },
        ],
    },
];

/// The anchor each industry is reached at, in catalog order.
///
/// Not in the catalogs: it appears in a URL, so it is the same in every
/// language. `/solutions#health` has to keep working when somebody sends it to
/// a colleague who reads the site in Chinese.
pub const INDUSTRY_SLUGS: [&str; 6] = [
    "health",
    "retail",
    "manufacturing",
    "services",
    "education",
    "non-profit",
];

/// Which plan is drawn as the emphasised column.
pub const PLAN_FEATURED: [bool; 3] = [false, true, false];

/// Whether the numbers on the pricing page are placeholders.
///
/// One switch rather than a flag per plan: nothing has been priced, so the
/// honest thing is for the page to say so once and to stop saying it the moment
/// a real number lands. See ADR 0007 section 12.
pub const PRICES_ARE_PROVISIONAL: bool = true;

/// One entry in the language switcher.
pub struct LanguageLink {
    pub code: &'static str,
    pub native_name: &'static str,
    /// The same page in that language, which is the only href worth offering.
    /// A switcher that always lands on the home page loses somebody's place
    /// every time they use it.
    pub href: String,
    /// The same address with the origin on the front, for `hreflang`.
    ///
    /// A relative `hreflang` is legal HTML and useless: the whole purpose of
    /// the tag is to tell a crawler that two *addresses* are one page, and a
    /// crawler that already knows the address it is on learns nothing from a
    /// path.
    pub absolute: String,
    pub current: bool,
}

/// The card a shared link unfurls into.
#[derive(Clone, Copy)]
pub struct SocialCard {
    pub url: &'static str,
    pub width: u32,
    pub height: u32,
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
    /// template rather than an enum: there are five, spelled once in
    /// `_nav.html`, and a page naming one that does not exist highlights
    /// nothing.
    pub current: &'static str,
    /// This page's one true address.
    ///
    /// Emitted as `<link rel="canonical">` and as `og:url`. Every page here is
    /// reachable at exactly one address, so this is never doing the job it is
    /// famous for - collapsing a page that answers on several. It is here for
    /// the ordinary reason instead: a link somebody shares acquires tracking
    /// parameters, and without this each variant is a separate page competing
    /// with the original.
    pub canonical: String,
    /// Where the site lives, for the few places a path will not do.
    pub origin: String,
    /// The picture a link unfurls into, when `artifacts/og-image` exists.
    pub social: Option<SocialCard>,
    /// The anchors the solutions panel links to, and the colour class each
    /// application's dot wears.
    ///
    /// On the frame because `_nav.html` is included by every page, so anything
    /// the navigation needs has to be somewhere every page already carries -
    /// the alternative is six page structs growing the same two fields.
    pub industry_slugs: [&'static str; 6],
    pub app_classes: [&'static str; 3],
    /// This page's title and one-sentence summary.
    ///
    /// Fields rather than template blocks, because `og:title` needs the same
    /// string `<title>` used and a block cannot be rendered twice. Set by each
    /// handler from its own section of the catalog - see [`Frame::titled`].
    pub title: &'static str,
    pub description: &'static str,
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
        let origin = state.origin().to_owned();

        let languages = state
            .languages()
            .iter()
            .map(|offered| {
                let href = address(*offered, path);

                LanguageLink {
                    code: offered.code(),
                    native_name: offered.native_name(),
                    absolute: format!("{origin}{href}"),
                    href,
                    current: *offered == language,
                }
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
            canonical: format!("{origin}{}", address(language, path)),
            social: artifacts::SOCIAL.as_ref().map(|card| SocialCard {
                url: card.url,
                width: card.width,
                height: card.height,
            }),
            origin,
            industry_slugs: INDUSTRY_SLUGS,
            app_classes: [
                APP_MARKS[0].class,
                APP_MARKS[1].class,
                APP_MARKS[2].class,
            ],
            // Overwritten by `titled` before the page renders. Empty rather
            // than a placeholder: a title that says "TODO" is worse in a search
            // result than one that is short.
            title: "",
            description: "",
        }
    }

    /// What this page is called, and what it is about.
    fn titled(mut self, title: &'static str, description: &'static str) -> Self {
        self.title = title;
        self.description = description;
        self
    }

    /// The structured data every page carries.
    ///
    /// An `Organization` and a `WebSite`, which is the honest amount: they are
    /// the two things this site can state about itself without inventing
    /// anything. There is deliberately no `Product` with an `offers` price on
    /// it, because the prices are placeholders (see
    /// [`PRICES_ARE_PROVISIONAL`]) and marking up a made-up number is how a
    /// search engine ends up quoting it.
    ///
    /// Rendered through askama's `json` filter, which emits no chevrons - so
    /// the string cannot close the script element it sits in.
    pub fn structured_data(&self) -> serde_json::Value {
        let mut organization = serde_json::json!({
            "@type": "Organization",
            "name": self.product_name,
            "url": self.origin,
            "description": self.description,
        });

        if let Some(card) = self.social
            && let Some(map) = organization.as_object_mut()
        {
            map.insert(
                "logo".to_owned(),
                serde_json::Value::String(format!("{}{}", self.origin, card.url)),
            );
        }

        serde_json::json!({
            "@context": "https://schema.org",
            "@graph": [
                organization,
                {
                    "@type": "WebSite",
                    "name": self.product_name,
                    "url": self.origin,
                    "inLanguage": self.lang,
                },
            ],
        })
    }
}

/// A page's address in one language.
///
/// `/` rather than the empty string for the home page: an `href=""` re-requests
/// the current URL including its query, which is not the same link.
pub fn address(language: Language, path: &str) -> String {
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

/// A screenshot with its caption and its alternative text already resolved.
///
/// Built per request rather than stored, because both strings are translated
/// and the picture is not.
pub struct Picture {
    pub url: &'static str,
    pub width: u32,
    pub height: u32,
    /// What the window chrome says above it. Product interface text - the
    /// workspace, the application, the screen - and deliberately not
    /// translated, like the labels on the drawn illustration.
    pub caption: String,
    pub alt: String,
}

impl Picture {
    /// Every screenshot for one application.
    ///
    /// `english` and `name` are the same application in two languages, and the
    /// two strings here want different ones. The caption is the window chrome
    /// over a picture of an English workspace, so it is English on every page -
    /// it is describing what is *in* the image, and the template marks it
    /// `lang="en"` accordingly. The alternative text describes that image to
    /// somebody who cannot see it, so it is in their language.
    fn of(app: &str, english: &str, name: &str, product: &str, pattern: &str) -> Vec<Self> {
        Shot::of(app)
            .map(|shot| {
                // `inventory-item-form` reads as "item form" once the
                // application is already named beside it.
                let screen = shot.screen.replace('-', " ");

                Self {
                    url: shot.url,
                    width: shot.width,
                    height: shot.height,
                    caption: format!("acme \u{b7} {english} \u{b7} {screen}"),
                    alt: pattern
                        .replace("{app}", name)
                        .replace("{product}", product)
                        .replace("{screen}", &screen),
                }
            })
            .collect()
    }
}

/// One application, with its mark, its slug and whatever pictures exist of it.
///
/// Assembled here so a template never has to index three parallel arrays and
/// hope they line up.
pub struct AppView {
    pub slug: &'static str,
    pub mark: &'static Mark,
    pub pictures: Vec<Picture>,
}

fn app_views(t: &'static Strings, product: &str) -> Vec<AppView> {
    // The English names, for the captions. See [`Picture::of`].
    let english = i18n::strings(Language::ENGLISH);

    APP_SLUGS
        .iter()
        .enumerate()
        .map(|(index, slug)| AppView {
            slug,
            mark: &APP_MARKS[index],
            pictures: Picture::of(
                slug,
                english.apps[index].name,
                t.apps[index].name,
                product,
                t.common.shot_alt,
            ),
        })
        .collect()
}

/// Every address the sitemap lists, without a language prefix.
///
/// One list, so a page that exists cannot be missing from the sitemap and an
/// address in the sitemap cannot 404. Both are things a crawler reports and
/// nobody reads.
pub const PATHS: [&str; 6] = ["", "/solutions", "/product", "/pricing", "/about", "/contact"];

// ---------------------------------------------------------------------------
// The pages
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomePage {
    pub frame: Frame,
    pub t: &'static Strings,
    pub apps: Vec<AppView>,
    /// The one screenshot the home page shows, if there is one. The illustration
    /// stands in for it when there is not - see `home.html`.
    pub lead: Option<Picture>,
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
    pub apps: Vec<AppView>,
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
    let mut apps = app_views(t, state.product_name());

    // The first picture of the first application that has one, promoted out of
    // its tile to stand as the page's single screenshot. Taken rather than
    // borrowed so the tile below does not show it twice.
    let lead = apps
        .iter_mut()
        .find(|app| !app.pictures.is_empty())
        .map(|app| app.pictures.remove(0));

    crate::routes::render(&HomePage {
        frame: Frame::new(state, language, t, "home", "").titled(t.home.title, t.home.description),
        t,
        apps,
        lead,
        trial_note: with_days(t.home.trial_note, state.config.desk.trial_days),
    })
}

pub async fn product(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&ProductPage {
        frame: Frame::new(state, language, t, "product", "/product")
            .titled(t.product.title, t.product.description),
        t,
        apps: app_views(t, state.product_name()),
    })
}

pub async fn pricing(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&PricingPage {
        frame: Frame::new(state, language, t, "pricing", "/pricing")
            .titled(t.pricing.title, t.pricing.description),
        t,
        featured: PLAN_FEATURED,
        trial_note: with_days(t.pricing.trial_note, state.config.desk.trial_days),
        provisional: PRICES_ARE_PROVISIONAL,
    })
}

pub async fn about(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&AboutPage {
        frame: Frame::new(state, language, t, "about", "/about")
            .titled(t.about.title, t.about.description),
        t,
    })
}

pub async fn contact(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&ContactPage {
        frame: Frame::new(state, language, t, "contact", "/contact")
            .titled(t.contact.title, t.contact.description),
        t,
    })
}

#[derive(Template)]
#[template(path = "solutions.html")]
pub struct SolutionsPage {
    pub frame: Frame,
    pub t: &'static Strings,
}

pub async fn solutions(state: &SiteState, language: Language) -> Response {
    let t = i18n::strings(language);

    crate::routes::render(&SolutionsPage {
        frame: Frame::new(state, language, t, "solutions", "/solutions")
            .titled(t.solutions.title, t.solutions.description),
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
        frame: Frame::new(state, language, t, "", "").titled(t.not_found.title, t.not_found.detail),
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
    fn the_marks_and_the_flags_match_the_catalogs() {
        let english = i18n::strings(Language::ENGLISH);

        assert_eq!(APP_MARKS.len(), english.apps.len());
        assert_eq!(APP_SLUGS.len(), english.apps.len());
        assert_eq!(PLAN_FEATURED.len(), english.plans.len());
        assert_eq!(INDUSTRY_SLUGS.len(), english.solutions.industries.len());
    }

    /// The build script checks a screenshot's filename against its own copy of
    /// this list, because a typo should fail when the file is added rather than
    /// when somebody notices the picture missing. Two copies, and this is what
    /// stops them drifting apart.
    #[test]
    fn the_slugs_match_the_build() {
        let script = std::fs::read_to_string("../../tools/site-artifacts.mjs")
            .expect("the build script is beside the crate");

        for slug in APP_SLUGS {
            assert!(
                script.contains(&format!("\"{slug}\"")),
                "tools/site-artifacts.mjs does not know about {slug}, so a \
                 {slug}-*.webp screenshot would fail the build"
            );
        }
    }

    /// An anchor is a URL, and a URL that changes breaks somebody else's link.
    #[test]
    fn an_industry_anchor_is_url_safe() {
        for slug in INDUSTRY_SLUGS {
            assert!(
                slug.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{slug} is not a safe anchor"
            );
        }
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
