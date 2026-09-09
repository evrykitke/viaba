//! The change trail: one row per change to one record.
//!
//! # Why this is not more events on the security trail
//!
//! `identity_events` grew a CRUD event at a time - `user_updated`,
//! `role_created`, `organization_profile_changed` - and each one cost a
//! migration restating a CHECK constraint across every tenant database. Two
//! things were wrong with that:
//!
//! * **Auditing a new entity was a schema change.** So the cheapest thing to do
//!   when adding an entity was not to audit it, which is exactly backwards.
//! * **Nothing in the row said which record it was.** The trail could answer
//!   "what happened on Tuesday" and never "what has ever happened to *this*
//!   role", which is the question a detail page is looking at when it asks.
//!
//! A change here names its record - [`EntityKind`] and an id - so both
//! questions are one index away, and adding an entity is a `const` in
//! [`kinds`] rather than a migration.
//!
//! # The vocabulary is declared here, not in SQL
//!
//! `entity_events.entity_type` has no CHECK constraint on purpose. This module
//! is what knows the set, because it is also what knows what to call each kind
//! on screen and where its record lives - and a constraint in the database
//! could only ever repeat that, one migration behind.
//!
//! The cost is that an unknown value can be read. That is handled rather than
//! prevented: a kind this build has never heard of renders as itself, which is
//! the right behaviour on a rolling deploy where a newer release is already
//! writing rows an older screen has to show.

use serde::{Deserialize, Serialize};

use super::change::{Fact, FieldChange};
use crate::i18n::{Catalog, datetime};
use crate::identity::user::UserId;
use crate::{Message, msg};

/// A kind of record the trail can talk about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityKind {
    /// Stored verbatim in `entity_events.entity_type`.
    ///
    /// Renaming one orphans its history, in the sense that the old rows keep
    /// the old name and stop being found by the new one. Treat it the way a
    /// permission name is treated: stable, and changed by a data migration if
    /// it must change at all.
    pub name: &'static str,
    /// The key for what one of them is called on screen.
    ///
    /// A key rather than the word, and a field rather than a `match`, because
    /// this is a const table: the words come out of the catalog at render time
    /// and `every_kind_names_itself_with_keys_that_exist` keeps the two in
    /// step. Read it through [`singular`](Self::singular).
    pub singular_key: &'static str,
    /// The key for what a list of them is called.
    pub plural_key: &'static str,
    /// Where the record itself lives, with `{id}` standing in for its id.
    ///
    /// `None` for a record with no page of its own. Used to turn a trail row
    /// back into the thing it is about, which is the first thing somebody
    /// wants after reading one.
    pub href: Option<&'static str>,
    /// One row, workspace-wide, with no id to point at.
    ///
    /// The organization's own details are the example: there is exactly one
    /// profile, and it records [`Self::singleton_id`] as its id so that the
    /// history section on the settings screen has something to look up.
    pub singleton: bool,
}

impl EntityKind {
    /// What one of them is called on screen.
    pub fn singular(&self) -> Message {
        Message::new(self.singular_key)
    }

    /// What a list of them is called.
    pub fn plural(&self) -> Message {
        Message::new(self.plural_key)
    }

    /// The id a singleton records: its own name.
    ///
    /// A real key would have to be invented, and an invented key is one that
    /// two call sites can spell differently. This one cannot be.
    pub const fn singleton_id(&self) -> &'static str {
        self.name
    }

    /// The record's own page, if it has one.
    pub fn href_for(&self, id: &str) -> Option<String> {
        self.href.map(|href| href.replace("{id}", id))
    }
}

/// Every kind of record this build audits.
///
/// Adding one is a `const` here plus the calls that record it - no migration,
/// because `entity_events.entity_type` is deliberately unconstrained. See the
/// module docs.
pub mod kinds {
    use super::EntityKind;

    /// The legal entity behind the workspace: its name, address, logo.
    ///
    /// A singleton, and the one whose changes reach furthest - these are what
    /// appear on everything the workspace issues.
    pub const ORGANIZATION: EntityKind = EntityKind {
        name: "organization",
        singular_key: "entity.organization.singular",
        plural_key: "entity.organization.plural",
        href: Some("/admin/settings?tab=organization"),
        singleton: true,
    };

    /// A way to hold permissions. Deleting one silently strips whatever it
    /// granted from everybody who held it, which is why its history matters.
    pub const ROLE: EntityKind = EntityKind {
        name: "role",
        singular_key: "entity.role.singular",
        plural_key: "entity.role.plural",
        href: Some("/admin/roles/{id}"),
        singleton: false,
    };

    /// An account: its name, its status, its roles, its own permissions.
    pub const USER: EntityKind = EntityKind {
        name: "user",
        singular_key: "entity.user.singular",
        plural_key: "entity.user.plural",
        href: Some("/admin/users/{id}/edit"),
        singleton: false,
    };

    /// What this workspace requires of the people in it - password rules and
    /// the two-factor policy, which are one row and one save.
    pub const SECURITY_POLICY: EntityKind = EntityKind {
        name: "security_policy",
        singular_key: "entity.security_policy.singular",
        plural_key: "entity.security_policy.plural",
        href: Some("/admin/settings?tab=security"),
        singleton: true,
    };

    /// Where this workspace's mail goes. The one setting that can redirect
    /// every invitation and every reset link to a relay somebody else controls.
    pub const MAIL_SETTINGS: EntityKind = EntityKind {
        name: "mail_settings",
        singular_key: "entity.mail_settings.singular",
        plural_key: "entity.mail_settings.plural",
        href: Some("/admin/settings?tab=communication"),
        singleton: true,
    };

    /// An organization or person the workspace trades with.
    ///
    /// Their name and address reach every document raised against them, and a
    /// bank detail or a tax registration changed quietly is the kind of edit a
    /// trail exists for.
    pub const PARTY: EntityKind = EntityKind {
        name: "party",
        singular_key: "entity.party.singular",
        plural_key: "entity.party.plural",
        href: Some("/master/parties/{id}"),
        singleton: false,
    };

    /// A tax, and what it is charged at.
    ///
    /// The rate windows are audited as part of the code rather than as their
    /// own kind: "VAT went from 17.5% to 20% on that date, and this is who
    /// entered it" is one story, and splitting it across two trails is how the
    /// second half stops being read.
    pub const TAX_CODE: EntityKind = EntityKind {
        name: "tax_code",
        singular_key: "entity.tax_code.singular",
        plural_key: "entity.tax_code.plural",
        href: Some("/master/taxes/{id}"),
        singleton: false,
    };

    /// What a document line actually references. Changing its membership
    /// changes what every future document using it comes to.
    pub const TAX_GROUP: EntityKind = EntityKind {
        name: "tax_group",
        singular_key: "entity.tax_group.singular",
        plural_key: "entity.tax_group.plural",
        href: Some("/master/tax-groups/{id}"),
        singleton: false,
    };

    /// Which currencies the workspace deals in, and what a rate was on a day.
    ///
    /// A singleton, because the interesting change is to the *list* rather than
    /// to one row: "who switched EUR off" and "who loaded Tuesday's rates" are
    /// the same question about the same screen.
    pub const CURRENCIES: EntityKind = EntityKind {
        name: "currencies",
        singular_key: "entity.currencies.singular",
        plural_key: "entity.currencies.plural",
        href: Some("/admin/settings?tab=currencies"),
        singleton: true,
    };

    /// An invoice: raised, posted, voided.
    ///
    /// The one kind here whose *status* changes are the interesting entries.
    /// "Who posted this, and when" is the question a document provokes, and
    /// posting is the act that cannot be undone.
    pub const SALES_INVOICE: EntityKind = EntityKind {
        name: "sales_invoice",
        singular_key: "entity.sales_invoice.singular",
        plural_key: "entity.sales_invoice.plural",
        href: Some("/sales/invoices/{id}"),
        singleton: false,
    };

    /// A posting to the general ledger.
    ///
    /// Recorded even though a journal is already append-only, because the trail
    /// answers a different question: the ledger says what was posted, and this
    /// says who posted it and from which screen. A reversal appears here as its
    /// own creation, which is what makes the pair readable as a correction
    /// rather than as an edit.
    pub const JOURNAL: EntityKind = EntityKind {
        name: "journal",
        singular_key: "entity.journal.singular",
        plural_key: "entity.journal.plural",
        href: Some("/sales/journals/{id}"),
        singleton: false,
    };

    /// A period of the accounting calendar, opened and closed.
    ///
    /// Closing is the strongest routine control there is - it is what makes a
    /// filed report stay filed - and reopening one is the act an auditor most
    /// wants to be able to see.
    pub const PERIOD: EntityKind = EntityKind {
        name: "period",
        singular_key: "entity.period.singular",
        plural_key: "entity.period.plural",
        href: Some("/sales/periods"),
        singleton: false,
    };

    /// One account in the chart, and what it is for.
    ///
    /// The change worth recording is the *type*: it decides the normal balance
    /// and which side of a report a balance lands on, so retyping an account
    /// restates every period it appears in. Renumbering is recorded for the
    /// same reason a rename is - somebody will ask why last year's export does
    /// not match this year's.
    pub const ACCOUNT: EntityKind = EntityKind {
        name: "account",
        singular_key: "entity.account.singular",
        plural_key: "entity.account.plural",
        href: Some("/sales/accounts/{id}"),
        singleton: false,
    };

    /// A part of the organization, and whether spending is charged to it.
    ///
    /// The audited change that matters is not the rename: it is
    /// `is_cost_centre`, because switching it on puts the department into every
    /// posting picker in the workspace and switching it off takes it out of
    /// them, and "who made this chargeable" is a question a cost report
    /// eventually asks.
    pub const DEPARTMENT: EntityKind = EntityKind {
        name: "department",
        singular_key: "entity.department.singular",
        plural_key: "entity.department.plural",
        href: Some("/people/departments/{id}"),
        singleton: false,
    };

    /// A person, and their employment.
    ///
    /// The trail matters more here than on most rows in this codebase, because
    /// the record is about somebody rather than about a thing: who changed a
    /// leaving date, who gave a person a login, and who moved them between
    /// departments are all questions with a person on the other end of them.
    ///
    /// Every dated act - hiring, moving, leaving, rehiring - is recorded
    /// against this one kind rather than against the engagement or the
    /// assignment, so one screen shows the whole of somebody's history in the
    /// order it happened.
    pub const EMPLOYEE: EntityKind = EntityKind {
        name: "employee",
        singular_key: "entity.employee.singular",
        plural_key: "entity.employee.plural",
        href: Some("/people/employees/{id}"),
        singleton: false,
    };

    /// A role the organization is made of.
    pub const JOB_POSITION: EntityKind = EntityKind {
        name: "job_position",
        singular_key: "entity.job_position.singular",
        plural_key: "entity.job_position.plural",
        href: Some("/people/roles/{id}"),
        singleton: false,
    };

    /// A place people work.
    pub const WORK_LOCATION: EntityKind = EntityKind {
        name: "work_location",
        singular_key: "entity.work_location.singular",
        plural_key: "entity.work_location.plural",
        href: Some("/people/places/{id}"),
        singleton: false,
    };

    /// An item: what the workspace stocks, buys and sells.
    ///
    /// Its `tracking` and its stock unit are the fields worth a trail. Both are
    /// refused once stock exists, so a change to either is a change made while
    /// the item was still empty - and "when did this stop being empty" is what
    /// somebody asks when a count comes out wrong.
    pub const ITEM: EntityKind = EntityKind {
        name: "item",
        singular_key: "entity.item.singular",
        plural_key: "entity.item.plural",
        href: Some("/inventory/items/{id}"),
        singleton: false,
    };

    /// An item category, and therefore a costing method.
    ///
    /// Changing `costing_method` restates what the workspace says its stock is
    /// worth. That is an accountant's decision made on an inventory screen, and
    /// it is the single strongest reason this kind exists.
    pub const ITEM_CATEGORY: EntityKind = EntityKind {
        name: "item_category",
        singular_key: "entity.item_category.singular",
        plural_key: "entity.item_category.plural",
        href: Some("/inventory/categories/{id}"),
        singleton: false,
    };

    /// A warehouse, and how many steps it receives and ships in.
    pub const WAREHOUSE: EntityKind = EntityKind {
        name: "warehouse",
        singular_key: "entity.warehouse.singular",
        plural_key: "entity.warehouse.plural",
        href: Some("/inventory/warehouses/{id}"),
        singleton: false,
    };

    /// A stock location.
    ///
    /// Recorded because a location's *kind* decides what a move across it
    /// means: turning one into a transit location changes whether stock in it
    /// is on hand, and the stock report changes shape without any stock moving.
    pub const STOCK_LOCATION: EntityKind = EntityKind {
        name: "stock_location",
        singular_key: "entity.stock_location.singular",
        plural_key: "entity.stock_location.plural",
        href: Some("/inventory/locations/{id}"),
        singleton: false,
    };

    /// A stock movement.
    ///
    /// Recorded even though the moves are themselves an append-only trail, and
    /// for a different question: the move says what happened to the stock, and
    /// this says who keyed it and when. A count difference somebody entered at
    /// half past six is a fact about a person, not about a shelf.
    pub const STOCK_MOVE: EntityKind = EntityKind {
        name: "stock_move",
        singular_key: "entity.stock_move.singular",
        plural_key: "entity.stock_move.plural",
        href: Some("/inventory/moves?id={id}"),
        singleton: false,
    };

    /// A requisition.
    ///
    /// The document posts nothing, and is recorded anyway: approving one is
    /// what lets money be spent against it, and "who said yes" is a fact about
    /// a person that the requisition row only holds until somebody edits it.
    /// The rejections matter as much as the approvals - a request that was
    /// turned down twice and granted on the third asking is a pattern no single
    /// row shows.
    pub const REQUISITION: EntityKind = EntityKind {
        name: "requisition",
        singular_key: "entity.requisition.singular",
        plural_key: "entity.requisition.plural",
        href: Some("/inventory/requisitions/{id}"),
        singleton: false,
    };

    /// A consolidation.
    ///
    /// Recorded because confirming one raises several purchase orders at once
    /// and decides which department's request each of them serves. The orders
    /// carry their own trail; this is the record of the single act that made
    /// them all, and of who chose the suppliers.
    pub const CONSOLIDATION: EntityKind = EntityKind {
        name: "consolidation",
        singular_key: "entity.consolidation.singular",
        plural_key: "entity.consolidation.plural",
        href: Some("/inventory/consolidations/{id}"),
        singleton: false,
    };

    /// A purchase order.
    ///
    /// Recorded because confirming one is a commitment to spend money, and
    /// "who agreed to this" is the first question asked about a delivery
    /// nobody remembers ordering.
    pub const PURCHASE_ORDER: EntityKind = EntityKind {
        name: "purchase_order",
        singular_key: "entity.purchase_order.singular",
        plural_key: "entity.purchase_order.plural",
        href: Some("/inventory/orders/{id}"),
        singleton: false,
    };

    /// A goods receipt.
    ///
    /// The document that puts stock on the balance sheet and creates a
    /// liability before the supplier's invoice has been seen. Who posted it,
    /// and when, is a fact about a person rather than about a shelf - which is
    /// why it is here as well as in the stock moves it created.
    /// A supplier bill. Recorded because posting one creates a payable, and a
    /// match that was overridden is a decision somebody made.
    pub const BILL: EntityKind = EntityKind {
        name: "bill",
        singular_key: "entity.bill.singular",
        plural_key: "entity.bill.plural",
        href: Some("/inventory/bills/{id}"),
        singleton: false,
    };

    /// A landed cost. Recorded because posting one changes what stock on the
    /// shelf is worth, and the delivery it names does not say who decided that.
    pub const LANDED_COST: EntityKind = EntityKind {
        name: "landed_cost",
        singular_key: "entity.landed_cost.singular",
        plural_key: "entity.landed_cost.plural",
        href: Some("/inventory/landed-costs/{id}"),
        singleton: false,
    };

    pub const RECEIPT: EntityKind = EntityKind {
        name: "goods_receipt",
        singular_key: "entity.goods_receipt.singular",
        plural_key: "entity.goods_receipt.plural",
        href: Some("/inventory/receipts/{id}"),
        singleton: false,
    };

    /// A unit of measure.
    ///
    /// The factor is the fact worth keeping. Every quantity ever recorded in
    /// this unit was recorded against the factor of the day, and changing it
    /// silently restates all of them.
    pub const UNIT_OF_MEASURE: EntityKind = EntityKind {
        name: "unit_of_measure",
        singular_key: "entity.unit_of_measure.singular",
        plural_key: "entity.unit_of_measure.plural",
        href: Some("/inventory/units"),
        singleton: false,
    };

    /// A document number series: its format, its reset period, where it starts.
    ///
    /// The one settings change that can make two documents share a number, so
    /// it is recorded per sequence rather than as one settings blob.
    pub const NUMBER_SEQUENCE: EntityKind = EntityKind {
        name: "number_sequence",
        singular_key: "entity.number_sequence.singular",
        plural_key: "entity.number_sequence.plural",
        href: Some("/admin/settings?tab=numbering"),
        singleton: false,
    };

    /// An app the workspace switched on or off.
    ///
    /// Not a singleton: which app is the whole question, and a subscription
    /// record that could not say *what* was subscribed to would be no record.
    /// The id is the app id, which is why the store page can be reached
    /// straight from a trail row.
    pub const APP: EntityKind = EntityKind {
        name: "app",
        singular_key: "entity.app.singular",
        plural_key: "entity.app.plural",
        href: Some("/admin/apps"),
        singleton: false,
    };
    /// A bearer credential for the API. Its history is the answer to "who
    /// could reach this workspace from outside, and when did that start".
    ///
    /// Issuing and revoking are recorded; using one is not. A row per request
    /// would drown the trail this exists to keep readable - what a key *did*
    /// shows up as the ordinary history of whatever it changed.
    pub const API_KEY: EntityKind = EntityKind {
        name: "api_key",
        singular_key: "entity.api_key.singular",
        plural_key: "entity.api_key.plural",
        href: Some("/admin/api-keys"),
        singleton: false,
    };
}

/// The declared set, in the order a filter should offer them.
pub const ENTITY_KINDS: &[EntityKind] = &[
    kinds::ORGANIZATION,
    kinds::USER,
    kinds::ROLE,
    kinds::SECURITY_POLICY,
    kinds::MAIL_SETTINGS,
    kinds::CURRENCIES,
    kinds::NUMBER_SEQUENCE,
    kinds::APP,
    kinds::API_KEY,
    kinds::PARTY,
    kinds::TAX_CODE,
    kinds::TAX_GROUP,
    kinds::SALES_INVOICE,
    kinds::ACCOUNT,
    kinds::JOURNAL,
    kinds::PERIOD,
    kinds::DEPARTMENT,
    kinds::EMPLOYEE,
    kinds::JOB_POSITION,
    kinds::WORK_LOCATION,
    kinds::ITEM,
    kinds::ITEM_CATEGORY,
    kinds::WAREHOUSE,
    kinds::STOCK_LOCATION,
    kinds::STOCK_MOVE,
    kinds::REQUISITION,
    kinds::CONSOLIDATION,
    kinds::PURCHASE_ORDER,
    kinds::BILL,
    kinds::RECEIPT,
    kinds::LANDED_COST,
    kinds::UNIT_OF_MEASURE,
];

/// The kind with this stored name, if this build knows it.
pub fn kind(name: &str) -> Option<EntityKind> {
    ENTITY_KINDS.iter().find(|kind| kind.name == name).copied()
}

/// What happened to the record.
///
/// Three verbs, and the column that holds them *is* check-constrained, unlike
/// the kind: a fourth verb would be a change to what this trail means rather
/// than an addition to a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityAction {
    Created,
    Updated,
    Deleted,
}

impl EntityAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
        }
    }

    /// The stored verb as a value.
    ///
    /// Anything else reads as an edit. The CHECK constraint makes that
    /// unreachable from this codebase; what it guards against is a row losing
    /// its place in a history because a string did not parse.
    pub fn from_stored(stored: &str) -> Self {
        match stored {
            "created" => Self::Created,
            "deleted" => Self::Deleted,
            _ => Self::Updated,
        }
    }

    /// The action on its own, for a badge beside a record's name.
    ///
    /// A noun in French - "Création", not "créé" - because a badge stands on
    /// its own with nothing to agree with.
    pub fn name(self) -> Message {
        match self {
            Self::Created => msg!("audit.action.created"),
            Self::Updated => msg!("audit.action.updated"),
            Self::Deleted => msg!("audit.action.deleted"),
        }
    }

    /// A heading naming what happened to one record.
    ///
    /// The whole heading, not the verb on its own. A verb that has to be glued
    /// to a noun by the caller is a verb that cannot agree with it, and French
    /// would need to know the record's gender to write "créé" or "créée" - so
    /// French takes a noun form instead, which the catalog is free to do
    /// because it owns the sentence rather than a fragment of it.
    pub fn headline(self, record: impl Into<String>) -> Message {
        match self {
            Self::Created => msg!("audit.headline.created", record = record.into()),
            Self::Updated => msg!("audit.headline.updated", record = record.into()),
            Self::Deleted => msg!("audit.headline.deleted", record = record.into()),
        }
    }
}

/// One change, as a line of the trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityChange {
    pub id: i64,
    /// The stored kind name. Stable; the label is not.
    pub entity_type: String,
    /// Which record. A singleton records its kind name - see
    /// [`EntityKind::singleton_id`].
    pub entity_id: String,
    pub action: EntityAction,
    /// What the record was called at the time.
    ///
    /// Stored rather than joined, so the row still names the thing after it has
    /// been deleted - which is the row most worth reading.
    pub label: Option<String>,
    pub actor_id: Option<UserId>,
    /// Who did it, as recorded then. Kept even when that account is later
    /// removed, which is the point of a trail.
    pub actor_email: Option<String>,
    pub ip: Option<String>,
    /// The diff in one line, already rendered.
    pub summary: Option<String>,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

impl EntityChange {
    /// The declared kind, when this build knows it.
    pub fn kind(&self) -> Option<EntityKind> {
        kind(&self.entity_type)
    }

    /// What to call this kind of record.
    ///
    /// Takes a catalog rather than returning a [`Message`] because the answer
    /// goes *inside* another sentence, and a message's arguments are strings.
    /// Anything that composes has to resolve as it composes.
    ///
    /// Falls back to the stored name with its underscores opened out, so a kind
    /// written by a newer release reads as something rather than as nothing.
    /// That fallback is deliberately untranslated: it is a column value, and
    /// inventing a translation for it would hide that this build does not know
    /// the kind.
    pub fn kind_label(&self, catalog: &Catalog) -> String {
        match self.kind() {
            Some(kind) => catalog.render(&kind.singular()),
            None => capitalise(&self.entity_type.replace('_', " ")),
        }
    }

    /// The record, named: `Role "Auditor"`, or just the kind when there is
    /// nothing to name it by.
    ///
    /// The quotation marks come from the catalog too - French sets off a name
    /// with guillemets and German with low-then-high marks, and a hard-coded
    /// `"` is an English typographic convention wearing punctuation's clothes.
    pub fn record(&self, catalog: &Catalog) -> String {
        let kind = self.kind_label(catalog);

        match &self.label {
            Some(label) if !label.is_empty() => catalog.render(&msg!(
                "audit.record.named",
                kind = kind,
                label = label.clone()
            )),
            _ => kind,
        }
    }

    /// The row's heading: `Role "Auditor" edited`.
    pub fn headline(&self, catalog: &Catalog) -> String {
        catalog.render(&self.action.headline(self.record(catalog)))
    }

    /// Where to go to see the record this is about.
    ///
    /// `None` after a deletion: there is nothing left at that address, and a
    /// link to it is a link to an error page.
    pub fn href(&self) -> Option<String> {
        if self.action == EntityAction::Deleted {
            return None;
        }

        self.kind()?.href_for(&self.entity_id)
    }

    /// What happened, as a sentence.
    ///
    /// The detail page shows this above the diff: it is the "who, when, from
    /// where" that a table of fields cannot say.
    /// The whole sentence is one key, with the actor, the record, the address
    /// and the moment dropped into it. Built from fragments it would read
    /// backwards in German, which puts the verb at the end - and `lower_first`,
    /// which this used to call, is wrong there outright, because German
    /// capitalises every noun wherever it sits in a sentence.
    ///
    /// The date comes through [`i18n::datetime`](crate::i18n::datetime), which
    /// takes both the month names *and* the order they are assembled in from
    /// the catalog - so a German row reads "23. August 2026 um 14:05 UTC"
    /// rather than an English date with a German sentence around it.
    pub fn narration(&self, catalog: &Catalog) -> String {
        let who = match self.actor_email.as_deref() {
            Some(email) => email.to_owned(),
            None => catalog.render(&msg!("audit.actor.unknown")),
        };

        let record = self.record(catalog);
        let when = datetime::moment_long(catalog, self.occurred_at);

        let message = match (self.action, self.ip.as_deref()) {
            (EntityAction::Created, None) => {
                msg!(
                    "audit.narration.created",
                    who = who,
                    record = record,
                    when = when
                )
            }
            (EntityAction::Created, Some(ip)) => msg!(
                "audit.narration.created_from",
                who = who,
                record = record,
                ip = ip.to_owned(),
                when = when
            ),
            (EntityAction::Updated, None) => {
                msg!(
                    "audit.narration.updated",
                    who = who,
                    record = record,
                    when = when
                )
            }
            (EntityAction::Updated, Some(ip)) => msg!(
                "audit.narration.updated_from",
                who = who,
                record = record,
                ip = ip.to_owned(),
                when = when
            ),
            (EntityAction::Deleted, None) => {
                msg!(
                    "audit.narration.deleted",
                    who = who,
                    record = record,
                    when = when
                )
            }
            (EntityAction::Deleted, Some(ip)) => msg!(
                "audit.narration.deleted_from",
                who = who,
                record = record,
                ip = ip.to_owned(),
                when = when
            ),
        };

        catalog.render(&message)
    }
}

/// One change, opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityChangeDetail {
    pub change: EntityChange,
    /// The browser the request came from, verbatim. Shown only here, never in
    /// the list: it is long, and it matters exactly once - when somebody is
    /// working out whether two changes were the same person.
    pub user_agent: Option<String>,
    /// Empty when the stored detail carried no before and after - a creation
    /// recorded without its values, or a row from a release that wrote less.
    pub changes: Vec<FieldChange>,
    pub facts: Vec<Fact>,
}

fn capitalise(text: &str) -> String {
    let mut owned = text.to_owned();

    if let Some(first) = owned.get_mut(0..1) {
        first.make_ascii_uppercase();
    }

    owned
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Built-in English, which is what these tests assert the words of.
    ///
    /// A test that reads the *catalog* is reading the same table the screen
    /// does, so a key renamed on one side and not the other fails here rather
    /// than in a browser.
    fn english() -> Catalog {
        Catalog::builtin(crate::Language::ENGLISH)
    }

    #[test]
    fn every_kind_names_itself_with_keys_that_exist() {
        // `msg!` cannot check these: the key is a struct field, not a literal
        // at the call site, so the const assertion has nothing to fire on. The
        // guard has to be a test, exactly as it is for the permission names.
        for kind in ENTITY_KINDS {
            for key in [kind.singular_key, kind.plural_key] {
                assert!(
                    crate::i18n::catalog::builtin_contains(key),
                    "{}: no such translation key: {key}",
                    kind.name,
                );
            }

            // A kind named by another kind's key is the failure this cannot
            // otherwise catch - both keys exist, and the screen reads wrongly.
            assert!(
                kind.singular_key
                    .starts_with(&format!("entity.{}.", kind.name)),
                "{} is named by {}",
                kind.name,
                kind.singular_key,
            );
        }
    }

    fn change(entity_type: &str, action: EntityAction) -> EntityChange {
        EntityChange {
            id: 1,
            entity_type: entity_type.to_owned(),
            entity_id: "8f2c".to_owned(),
            action,
            label: None,
            actor_id: None,
            actor_email: None,
            ip: None,
            summary: None,
            occurred_at: chrono::DateTime::from_timestamp(1_770_000_000, 0).unwrap_or_default(),
        }
    }

    #[test]
    fn every_declared_kind_is_findable_by_the_name_it_stores() {
        // The catalogue and the lookup are the two halves of the same fact;
        // a kind missing from ENTITY_KINDS is one whose rows render as the
        // fallback forever.
        for kind in ENTITY_KINDS {
            assert_eq!(
                super::kind(kind.name).map(|found| found.name),
                Some(kind.name)
            );
        }
    }

    #[test]
    fn no_two_kinds_share_a_stored_name() {
        // Two kinds under one name is two histories in one list, and the
        // second one silently wins the lookup.
        let mut names: Vec<&str> = ENTITY_KINDS.iter().map(|kind| kind.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();

        assert_eq!(names.len(), before);
    }

    #[test]
    fn a_kind_this_build_does_not_know_still_reads_as_something() {
        // The rolling-deploy case: a newer release is already writing rows an
        // older screen has to show.
        assert_eq!(
            change("purchase_order", EntityAction::Updated).kind_label(&english()),
            "Purchase order"
        );
    }

    #[test]
    fn a_record_is_named_by_what_it_was_called_at_the_time() {
        let deleted = EntityChange {
            label: Some("Auditor".into()),
            ..change("role", EntityAction::Deleted)
        };

        assert_eq!(deleted.record(&english()), "Role \"Auditor\"");
        assert_eq!(deleted.headline(&english()), "Role \"Auditor\" deleted");
    }

    #[test]
    fn a_record_with_no_name_is_still_named_by_its_kind() {
        assert_eq!(
            change("role", EntityAction::Updated).record(&english()),
            "Role"
        );
    }

    #[test]
    fn a_deleted_record_has_nowhere_to_link_to() {
        // The record is gone; the link would be a link to an error page.
        assert!(change("role", EntityAction::Deleted).href().is_none());
        assert_eq!(
            change("role", EntityAction::Updated).href().as_deref(),
            Some("/admin/roles/8f2c"),
        );
    }

    #[test]
    fn a_singleton_links_to_the_screen_it_is_edited_on() {
        let profile = EntityChange {
            entity_id: kinds::ORGANIZATION.singleton_id().to_owned(),
            ..change("organization", EntityAction::Updated)
        };

        assert_eq!(
            profile.href().as_deref(),
            Some("/admin/settings?tab=organization")
        );
    }

    #[test]
    fn a_kind_this_build_does_not_know_has_nowhere_to_link_to() {
        // A name no `kinds` const uses, and one nothing is likely to claim -
        // this fixture named `purchase_order` until that became a real kind,
        // at which point the test was asserting the opposite of its own name.
        assert!(
            change("a_kind_from_a_newer_release", EntityAction::Updated)
                .href()
                .is_none()
        );
    }

    #[test]
    fn a_change_narrates_as_a_sentence() {
        let edited = EntityChange {
            label: Some("Auditor".into()),
            actor_email: Some("ada@example.test".into()),
            ip: Some("203.0.113.7".into()),
            ..change("role", EntityAction::Updated)
        };

        // Note the capital R. The sentence no longer lower-cases the kind on
        // its way in - that was an English typographic habit, and German
        // capitalises a noun wherever it sits.
        assert_eq!(
            edited.narration(&english()),
            "ada@example.test edited Role \"Auditor\" from 203.0.113.7 \
             on 2 February 2026 at 02:40 UTC.",
        );
    }

    #[test]
    fn a_change_with_no_account_behind_it_still_names_somebody() {
        assert!(
            change("role", EntityAction::Created)
                .narration(&english())
                .starts_with("Somebody created")
        );
    }

    #[test]
    fn a_stored_verb_this_build_cannot_read_is_an_edit_rather_than_a_lost_row() {
        assert_eq!(EntityAction::from_stored("created"), EntityAction::Created);
        assert_eq!(EntityAction::from_stored("deleted"), EntityAction::Deleted);
        assert_eq!(
            EntityAction::from_stored("transmogrified"),
            EntityAction::Updated
        );
    }

    #[test]
    fn the_stored_verbs_are_the_ones_the_check_constraint_allows() {
        // `entity_events_action_valid` in migration 0011. A fourth value here
        // would be an insert that fails at runtime and nowhere earlier.
        for action in [
            EntityAction::Created,
            EntityAction::Updated,
            EntityAction::Deleted,
        ] {
            assert!(matches!(action.as_str(), "created" | "updated" | "deleted"));
            assert_eq!(EntityAction::from_stored(action.as_str()), action);
        }
    }
}
