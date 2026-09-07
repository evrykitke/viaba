//! English, and the original every other catalog is a translation of.
//!
//! When a sentence changes, it changes here first - and the compiler will not
//! remind you about the others. They still compile, saying the old thing. That
//! is the one hole the type system does not close here: it catches a *missing*
//! sentence, never a stale one.

use super::{
    About, AppCopy, Beneath, Common, Contact, Footer, Home, Nav, NotFound, PillarCopy, PlanCopy,
    Pricing, Product, Strings,
};

pub static STRINGS: Strings = Strings {
    code: "en",

    common: Common {
        start_free: "Start free",
        get_started: "Get started",
        sign_in: "Sign in",
        talk_to_us: "Talk to us",
        see_inside: "See what's inside",
        tagline: "Making good management software accessible to everyone.",
        skip_to_content: "Skip to content",
        menu: "Menu",
        language: "Language",
        home_of: "home",
    },

    nav: Nav {
        product: "Product",
        pricing: "Pricing",
        about: "About",
        contact: "Contact",
        main: "Main",
    },

    footer: Footer {
        product: "Product",
        company: "Company",
        account: "Account",
        whats_inside: "What's inside",
        privacy: "Privacy",
        terms: "Terms",
        rights: "Built for the people who have to make the numbers agree.",
    },

    home: Home {
        title: "Making good management software accessible to everyone",
        description: "Accounting, stock and people in one workspace \u{2014} modelled the way the work actually happens.",
        eyebrow: "One workspace. Every part of the business.",
        headline_lead: "Making good management software",
        headline_accent: "accessible to everyone.",
        lede: "Accounting, stock and people in one place \u{2014} modelled the way the work actually happens, priced so a small company can start today, and built to still make sense when it is a large one.",
        trial_note: "{days} days on the house. No card, and your own workspace on its own address.",
        apps_title: "Every part, and only the parts you want.",
        apps_lede: "Each application is a separate piece of the product. Turn one on the day it starts to matter, and leave the rest off until then.",
        apps_more: "Purchasing, sales, projects and payroll are on the way, and each arrives as another part rather than another product to buy.",
        dense_title: "Dense where it should be dense.",
        dense_lede: "A day of work is spent in grids and forms, not on a landing page. The screens are built for that: compact rows, keyboard-first, and a number never more than a glance away.",
        reasons_title: "Built the way the work is shaped.",
        cta_title: "Your workspace takes about a minute.",
        cta_body: "Pick a name, pick an address, and it is yours \u{2014} its own database, its own users, its own permissions. Nothing shared with anybody else's.",
    },

    product: Product {
        title: "What's inside",
        description: "Accounting, stock and people \u{2014} one workspace, and only the parts you turn on.",
        eyebrow: "The product",
        headline: "One workspace, and only the parts you turn on.",
        lede: "Each application below is a separate piece. They share one set of users, one set of permissions and one ledger, and none of them needs the others to be useful.",
        underneath: "And underneath all three",
        beneath: [
            Beneath {
                heading: "Your own database",
                body: "One workspace, one database, one address. Not a column on a shared table.",
            },
            Beneath {
                heading: "Permissions that mean it",
                body: "Every screen and every action is gated, and the gate is checked on the server.",
            },
            Beneath {
                heading: "An audit trail",
                body: "Who changed what, and when. Written as it happens, not reconstructed after.",
            },
            Beneath {
                heading: "Four languages",
                body: "English, German, French and Chinese, checked at build time to stay in step.",
            },
        ],
        cta_title: "See it with your own data.",
        cta_body: "A workspace takes about a minute, and it is yours to keep.",
    },

    pricing: Pricing {
        title: "Pricing",
        description: "Start free, and pay per person when it becomes the way you run the business.",
        eyebrow: "Pricing",
        headline: "Accessible is part of the promise.",
        lede: "Start free and stay free while you are small. Pay for the people who use it, not for the modules you switched on.",
        most: "Most companies",
        trial_note: "Every plan starts with {days} days of the full product, and no card.",
        provisional_lead: "These figures are not final.",
        provisional_body: "The plans are the shape we are building towards; the prices are still being settled and will be published here before anybody is asked to pay one.",
        faq_title: "The questions people ask first",
        faq: [
            Beneath {
                heading: "What happens when the trial ends?",
                body: "The workspace stops serving and nothing is deleted. Your data stays where it is until you pick a plan or ask us to remove it.",
            },
            Beneath {
                heading: "Do I pay per application?",
                body: "No. You pay for the people who sign in. Turning on inventory does not change the bill.",
            },
            Beneath {
                heading: "Can I run it myself?",
                body: "Yes, on the Enterprise plan. It is one binary and a database, which is deliberate.",
            },
            Beneath {
                heading: "Is my data mixed in with everyone else's?",
                body: "No. Every workspace gets its own database on its own address. That is the design, not a paid upgrade.",
            },
        ],
    },

    about: About {
        title: "About",
        description: "Why we are building management software that a small company can actually afford to run.",
        eyebrow: "About",
        headline: "Good software should not be a thing only large companies get.",
        body: [
            "Most businesses run on a spreadsheet that one person understands, or on a system that cost more to configure than it did to buy. Both are the same problem wearing different clothes: the software was never shaped like the work, so somebody has to hold the difference in their head.",
            "We are building the other thing. Stock moves between locations because that is what stock does. A journal balances before it posts because that is what a journal is. When the software models the work, the person doing the work can be taught it in an afternoon \u{2014} and that is what makes it accessible, far more than the price does.",
            "The price matters too. Every workspace gets its own database, its own address and its own permissions, and none of that is held back as an upgrade. Small companies get the same product as large ones, because building two products is how the small one ends up being the bad one.",
        ],
        values_title: "What we hold to",
        cta_title: "Have a look for yourself.",
        cta_body: "The fastest way to judge any of this is to open it.",
    },

    contact: Contact {
        title: "Contact",
        description: "Talk to a person, or go straight to a workspace of your own.",
        eyebrow: "Contact",
        headline: "Ask us anything.",
        lede: "Whether it is a question about how something is modelled or whether it fits the way you work \u{2014} a real answer beats a demo.",
        cards: [
            Beneath {
                heading: "Send an email",
                body: "The quickest way to reach somebody who can answer properly.",
            },
            Beneath {
                heading: "Start a workspace",
                body: "About a minute, no card, and yours to keep afterwards.",
            },
            Beneath {
                heading: "Sign in",
                body: "Already have a workspace? It lives at its own address.",
            },
        ],
    },

    not_found: NotFound {
        title: "Not found",
        heading: "There is no page at that address.",
        detail: "The link may be old, or it may have been mistyped.",
    },

    apps: [
        AppCopy {
            name: "Books",
            tagline: "Double entry, the way an accountant would recognise it.",
            points: &[
                "A chart of accounts you can shape, with the roles the ledger needs",
                "Journals that balance before they post, and never after",
                "Financial years and periods that open and close deliberately",
                "Multi-currency with a rate on file, or the entry is refused",
            ],
            status: None,
        },
        AppCopy {
            name: "Inventory",
            tagline: "Stock as movements between places, not a number in a column.",
            points: &[
                "Items, variants and the units they are counted in",
                "Warehouses, locations and the moves between them",
                "Categories that decide costing, valuation and removal",
                "Lots and serials, with expiry where it matters",
            ],
            status: Some("Growing"),
        },
        AppCopy {
            name: "People",
            tagline: "Who works here, and which part of the business they cost to.",
            points: &[
                "Departments, and the ones that are cost centres",
                "A cost centre is a dimension on a journal line, not a report filter",
            ],
            status: Some("Early"),
        },
    ],

    pillars: [
        PillarCopy {
            heading: "It models the real thing",
            body: "Stock moves between locations because that is what stock does. A journal balances before it posts because that is what a journal is. Software that models the work is software you can explain to the person doing it.",
        },
        PillarCopy {
            heading: "Your workspace is yours",
            body: "One database per workspace, on its own address, with its own users and its own permissions. Not a tenant column on a shared table with a filter everybody has to remember.",
        },
        PillarCopy {
            heading: "Add only what you need",
            body: "Every application is a separate part. Take accounting without inventory, or inventory without payroll, and turn the next one on the day it starts to matter.",
        },
        PillarCopy {
            heading: "Fast where you work",
            body: "Dense screens, keyboard-first grids, and a compact type scale built for a day of use rather than a screenshot. Nothing here is waiting on a spinner to tell you a number you already knew.",
        },
    ],

    plans: [
        PlanCopy {
            name: "Starter",
            who: "One team finding its feet.",
            price: "Free",
            cadence: "while you are getting started",
            points: &[
                "One workspace",
                "Up to three people",
                "Books and Inventory",
                "Community support",
            ],
            action: "Start free",
        },
        PlanCopy {
            name: "Business",
            who: "A company running on it every day.",
            price: "\u{2014}",
            cadence: "per person, per month",
            points: &[
                "Everything in Starter",
                "Unlimited people",
                "Every application",
                "Multi-currency and tax",
                "Email support",
            ],
            action: "Start free",
        },
        PlanCopy {
            name: "Enterprise",
            who: "Several entities, and rules of your own.",
            price: "Let's talk",
            cadence: "",
            points: &[
                "Everything in Business",
                "Several workspaces",
                "Your own deployment",
                "Onboarding and migration",
            ],
            action: "Talk to us",
        },
    ],
};
