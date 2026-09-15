//! What a document does to the ledger.
//!
//! # The rule, not the writing of it
//!
//! This module says which accounts a sales invoice touches and which way. It
//! does not resolve a role to an account, look up an exchange rate, take a
//! number or open a transaction - all of that is the service's, and none of it
//! can be done in a browser. What is here is the part worth being sure about,
//! which is the part that is arithmetic and signs.
//!
//! # Why an invoice names roles rather than accounts
//!
//! Because the chart belongs to the workspace. "Trade receivables" is 1100 in
//! the chart this codebase seeds and something else in the next one, and a
//! document that stored the number would have to be rewritten when somebody
//! renumbers. So the posting names [`AccountRole::AccountsReceivable`] and the
//! ledger decides - the same mechanism Inventory reaches Books through, used
//! here by Books on its own document, because a second mechanism for the same
//! decision is a second place for it to be made differently.
//!
//! # Tax is not income
//!
//! An invoice's gross is what the customer owes; its net is what the workspace
//! earned; the difference was never the workspace's money and is owed to
//! whoever collects it. Three figures, three lines, and the reason a profit and
//! loss built on the net is the only one that is true.

use phonix_ports::ledger::{AccountRole, JournalRequest, Posting, Side};

use crate::invoice::{Invoice, InvoiceStatus};

/// Every posting a sales invoice makes.
///
/// ```text
///   DR  accounts receivable   gross
///       CR  revenue                   net
///       CR  tax payable               tax
/// ```
///
/// `None` where there is nothing to post. A zero invoice is a legitimate
/// document - a replacement at no charge, a sample - and the ledger refuses a
/// line of nothing, so the honest answer is that it moves no money rather than
/// a journal of three zeroes.
///
/// The tax line is dropped the same way, and for the same reason: an invoice
/// outside the scope of tax has no tax account to credit and must not credit
/// one with nothing.
///
/// Only a posted invoice. A draft is not a claim on anybody, and a voided one
/// has been withdrawn - its entry is reversed rather than re-derived.
pub fn sales_invoice(invoice: &Invoice) -> Option<JournalRequest> {
    if invoice.status != InvoiceStatus::Posted {
        return None;
    }

    let totals = &invoice.totals;
    if totals.gross.is_zero() {
        return None;
    }

    let memo = || Some(invoice.party.name.clone());

    // A credit note is this entry backwards. The amounts are the same positive
    // figures the document shows; only the sides move, which is what `kind` is
    // for - see `migrations/apps/books/0010`.
    let (owed, earned) = if invoice.kind.is_credit_note() {
        (Side::Credit, Side::Debit)
    } else {
        (Side::Debit, Side::Credit)
    };

    let mut postings = vec![Posting {
        role: AccountRole::AccountsReceivable,
        account_id: None,
        side: owed,
        amount: totals.gross.to_storage_string(),
        memo: memo(),
        cost_centre_id: None,
    }];

    if !totals.net.is_zero() {
        postings.push(Posting {
            role: AccountRole::Revenue,
            account_id: None,
            side: earned,
            amount: totals.net.to_storage_string(),
            memo: memo(),
            cost_centre_id: None,
        });
    }

    if !totals.tax.is_zero() {
        postings.push(Posting {
            role: AccountRole::TaxPayable,
            account_id: None,
            side: earned,
            amount: totals.tax.to_storage_string(),
            memo: memo(),
            cost_centre_id: None,
        });
    }

    Some(JournalRequest {
        entry_date: invoice.issued_on,
        // The document's own number and the customer it was sent to. What
        // somebody reading the ledger needs in order to find the paper.
        narration: narration(invoice),
        source_app: crate::APP_ID.to_owned(),
        source_doc_type: crate::journal::doc_types::SALES_INVOICE.to_owned(),
        source_doc_id: invoice.id,
        currency: invoice.currency.code().to_owned(),
        postings,
    })
}

/// What the journal is called in the ledger.
///
/// Not a translated string. A narration is stored once, read by whoever opens
/// the journal years later, and rendering it in the language of whoever
/// happened to press the button would make the ledger a record of who was on
/// duty.
fn narration(invoice: &Invoice) -> String {
    match &invoice.number {
        Some(number) => format!("{number} \u{b7} {}", invoice.party.name),
        None => invoice.party.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use crate::invoice::InvoiceKind;
    use chrono::NaiveDate;
    use phonix_core::locale::Currency;
    use phonix_core::money::Money;
    use phonix_master::address::PostalAddress;
    use uuid::Uuid;

    use super::*;
    use crate::invoice::{InvoiceTotals, PartySnapshot};

    const CURRENCY: Currency = Currency::USD;

    fn money(amount: &str) -> Money {
        Money::parse(CURRENCY, amount).unwrap()
    }

    fn invoice(net: &str, tax: &str, gross: &str) -> Invoice {
        let now = chrono::Utc::now();

        Invoice {
            id: Uuid::from_u128(42),
            number: Some("INV-2026-00042".to_owned()),
            kind: InvoiceKind::SalesInvoice,
            credits_invoice_id: None,
            status: InvoiceStatus::Posted,
            party: PartySnapshot {
                party_id: Uuid::from_u128(7),
                code: "ACME01".to_owned(),
                name: "Acme Fasteners Ltd".to_owned(),
                tax_id: None,
                address: PostalAddress::default(),
            },
            issued_on: NaiveDate::from_ymd_opt(2026, 3, 14).unwrap(),
            due_on: None,
            currency: CURRENCY,
            rate: None,
            pricing: phonix_tax::compute::Pricing::Exclusive,
            rounding_level: phonix_tax::compute::RoundingLevel::Line,
            rounding: phonix_core::money::Rounding::HalfUp,
            totals: InvoiceTotals {
                net: money(net),
                tax: money(tax),
                gross: money(gross),
                base_gross: None,
            },
            notes: None,
            lines: Vec::new(),
            posted_at: Some(now),
            posted_by: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// The debit is the whole claim and the two credits are what it is made of.
    #[test]
    fn the_customer_owes_the_gross_and_the_workspace_earned_the_net() {
        let request = sales_invoice(&invoice("1000.00", "200.00", "1200.00")).unwrap();

        let debits: Vec<_> = request
            .postings
            .iter()
            .filter(|posting| posting.side == Side::Debit)
            .collect();

        assert_eq!(debits.len(), 1);
        assert_eq!(debits[0].role, AccountRole::AccountsReceivable);
        assert_eq!(debits[0].amount, "1200.0000");

        let credits: Vec<_> = request
            .postings
            .iter()
            .filter(|posting| posting.side == Side::Credit)
            .map(|posting| (posting.role, posting.amount.as_str()))
            .collect();

        assert_eq!(
            credits,
            vec![
                (AccountRole::Revenue, "1000.0000"),
                (AccountRole::TaxPayable, "200.0000"),
            ]
        );
    }

    /// The ledger checks this too, and refuses what does not balance. Checking
    /// it here is how the refusal is never reached.
    #[test]
    fn the_two_sides_agree() {
        let request = sales_invoice(&invoice("1000.00", "200.00", "1200.00")).unwrap();

        let sum = |side: Side| {
            request
                .postings
                .iter()
                .filter(|posting| posting.side == side)
                .map(|posting| Money::parse(CURRENCY, &posting.amount).unwrap())
                .try_fold(Money::zero(CURRENCY), Money::checked_add)
                .unwrap()
        };

        assert_eq!(sum(Side::Debit), sum(Side::Credit));
    }

    /// A line of nothing is refused by the ledger, so it is never offered one.
    #[test]
    fn an_invoice_outside_the_scope_of_tax_credits_no_tax_account() {
        let request = sales_invoice(&invoice("1000.00", "0.00", "1000.00")).unwrap();

        assert_eq!(request.postings.len(), 2);
        assert!(
            !request
                .postings
                .iter()
                .any(|posting| posting.role == AccountRole::TaxPayable)
        );
    }

    /// A replacement at no charge is a document, and it moves no money.
    #[test]
    fn an_invoice_for_nothing_posts_nothing() {
        assert!(sales_invoice(&invoice("0.00", "0.00", "0.00")).is_none());
    }

    /// A draft is not a claim on anybody.
    #[test]
    fn a_draft_posts_nothing() {
        let mut draft = invoice("1000.00", "200.00", "1200.00");
        draft.status = InvoiceStatus::Draft;

        assert!(sales_invoice(&draft).is_none());
    }

    #[test]
    fn a_credit_note_is_the_invoice_entry_backwards() {
        // Same three roles, same positive amounts, every side the other way.
        // What the customer owes goes down, what was earned goes down, and the
        // tax that was never the workspace's goes back.
        let mut note = invoice("1000.0000", "200.0000", "1200.0000");
        note.kind = InvoiceKind::CreditNote;

        let entry = sales_invoice(&note).expect("a journal");

        let side_of = |role: AccountRole| {
            entry
                .postings
                .iter()
                .find(|posting| posting.role == role)
                .map(|posting| posting.side)
        };

        assert_eq!(side_of(AccountRole::AccountsReceivable), Some(Side::Credit));
        assert_eq!(side_of(AccountRole::Revenue), Some(Side::Debit));
        assert_eq!(side_of(AccountRole::TaxPayable), Some(Side::Debit));

        // Positive, not negative: the document says three widgets, not minus
        // three, and the sides carry the direction.
        for posting in &entry.postings {
            assert!(!posting.amount.starts_with('-'), "{}", posting.amount);
        }
    }

    #[test]
    fn a_credit_note_still_balances() {
        let mut note = invoice("1000.0000", "200.0000", "1200.0000");
        note.kind = InvoiceKind::CreditNote;

        let entry = sales_invoice(&note).expect("a journal");

        let sum = |side: Side| {
            entry
                .postings
                .iter()
                .filter(|posting| posting.side == side)
                .filter_map(|posting| posting.amount.parse::<f64>().ok())
                .sum::<f64>()
        };

        assert_eq!(sum(Side::Debit), sum(Side::Credit));
    }

    #[test]
    fn the_two_kinds_draw_from_different_series() {
        assert_ne!(
            InvoiceKind::SalesInvoice.series(),
            InvoiceKind::CreditNote.series()
        );
    }

    #[test]
    fn every_kind_round_trips() {
        for kind in InvoiceKind::ALL {
            assert_eq!(InvoiceKind::parse(kind.as_str()), Some(*kind));
        }

        assert_eq!(InvoiceKind::parse("not_a_kind"), None);
    }
}
