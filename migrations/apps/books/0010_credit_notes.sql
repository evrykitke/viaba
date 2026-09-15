-- books 0010: the document that takes an invoice back.
--
-- A credit note is an invoice of a different kind rather than a table of its
-- own. It has the same lines, the same tax snapshot, the same party snapshot
-- and the same numbering; what differs is which way its journal goes. ERPNext
-- makes it a Sales Invoice with `is_return`, and Odoo an `account.move` of
-- another type - both for the reason that matters here, which is that a second
-- table would be the invoice machinery written twice and reports reading both.
--
-- AMOUNTS STAY POSITIVE
--
-- A credit note for three widgets says three, not minus three. That is what the
-- document reads like to the person holding it, and it is what `kind` is for:
-- the posting flips the sides, and a report that sums money asks the kind
-- rather than trusting a sign somebody could have typed.

ALTER TABLE invoices
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'sales_invoice';

ALTER TABLE invoices
    ADD CONSTRAINT invoices_kind_known
        CHECK (kind IN ('sales_invoice', 'credit_note'));

COMMENT ON COLUMN invoices.kind IS
    'sales_invoice or credit_note. Decides which way the journal goes; amounts are positive either way.';

-- Which invoice this credits, where it credits one. A credit note may stand
-- alone - a goodwill payment, a correction to a customer with no open invoice -
-- so this is nullable rather than the document being defined by its parent.
ALTER TABLE invoices
    ADD COLUMN credits_invoice_id UUID REFERENCES invoices (id) ON DELETE RESTRICT;

COMMENT ON COLUMN invoices.credits_invoice_id IS
    'The invoice this credit note is against, where it is against one. Same table, same app, so this is a real key.';

ALTER TABLE invoices
    ADD CONSTRAINT invoices_credits_only_a_credit_note
        CHECK (credits_invoice_id IS NULL OR kind = 'credit_note');

CREATE INDEX invoices_credits ON invoices (credits_invoice_id)
    WHERE credits_invoice_id IS NOT NULL;
