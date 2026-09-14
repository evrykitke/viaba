-- books 0008: a customer paying.
--
-- The thing this ledger has never recorded. Posting an invoice creates a
-- receivable and nothing in the workspace could ever take it off again, so
-- "what are we owed" was the sum of every invoice ever raised, the ageing
-- ladder aged documents that had been settled for months, and the customer
-- statement said so on its own face because there was nothing else it could
-- honestly say.
--
--   payments              money moving, and where it landed
--   payment_allocations   which invoices it settled, and by how much
--
-- TWO TABLES, NOT A COLUMN ON THE INVOICE
--
-- A `paid_amount` on the invoice would be the obvious shortcut and it is the
-- one that cannot answer any of the questions that follow: one cheque settles
-- four invoices, one invoice is settled by three instalments, and a payment on
-- account settles nothing yet. Allocation is a relation, and a relation stored
-- as a running total on one side is a relation that stops reconciling the first
-- time anything is corrected.
--
-- MONEY ON ACCOUNT IS THE DIFFERENCE, NOT A ROW
--
-- A payment whose allocations come to less than its amount has the rest sitting
-- on the customer's account. That is arithmetic over the two tables, so there
-- is nothing to keep in step and nothing to forget to write - the same argument
-- `purchase_order_lines.received` makes about a backorder.
--
-- DIRECTION IS HERE AND ONLY ONE VALUE IS BUILT
--
-- `in` is a customer paying. `out` is this workspace paying a supplier, which
-- is the mirror and is not built: nothing raises it, no screen offers it, and
-- the service refuses it. The column exists because a table called `payments`
-- that can only be a receipt is a table named wrongly, and because the day the
-- purchase ledger wants one it should be a value rather than a second table
-- with the same six columns.

CREATE TABLE payments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `RCT-2026-00042`. NULL while it is a draft, on exactly the terms an
    -- invoice's number is: allocated at post, in the same transaction as the
    -- write, so a failed post returns it.
    number      TEXT,

    -- draft | posted | voided. The invoice's three states, and for the same
    -- reasons: a draft is editable and counts towards nothing, a posted one is
    -- evidence, and a voided one keeps its number because a number that
    -- disappears is a gap.
    status      TEXT NOT NULL DEFAULT 'draft',

    -- in | out. See the header.
    direction   TEXT NOT NULL DEFAULT 'in',

    -- A `master.parties` id with NO foreign key behind it, with the code and
    -- name snapshotted at post - exactly as `invoices` carries one.
    party_id    UUID NOT NULL,
    party_code  TEXT NOT NULL DEFAULT '',
    party_name  TEXT NOT NULL DEFAULT '',

    -- The day the money moved, which is the day the journal takes. Not the day
    -- somebody keyed it.
    received_on DATE NOT NULL,

    -- Where it landed: a bank or cash account in this workspace's own chart.
    -- A real foreign key, unlike the party: this is Books' own table pointing
    -- at Books' own accounts, and RESTRICT because an account money has gone
    -- through may not be deleted out from under it.
    account_id  UUID NOT NULL REFERENCES accounts (id) ON DELETE RESTRICT,

    -- The six-column currency snapshot, per ADR 0001 section 3. `amount` is as
    -- received; the rest is what it was worth in the workspace's own currency
    -- on the day, recorded so nothing is recomputed from a later rate. NULL
    -- where the payment is already in the base currency, on the same terms as
    -- an invoice's.
    currency_code      TEXT NOT NULL REFERENCES core.currencies (code),
    amount             NUMERIC(19, 4) NOT NULL,
    base_currency_code TEXT REFERENCES core.currencies (code),
    exchange_rate      NUMERIC(20, 10),
    rate_date          DATE,
    base_amount        NUMERIC(19, 4),

    -- Their cheque number, the transfer reference, the last four digits of the
    -- card. What somebody matches against a bank statement.
    reference   TEXT,
    note        TEXT,

    posted_at   TIMESTAMPTZ,
    posted_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT payments_status_valid CHECK (status IN ('draft', 'posted', 'voided')),
    CONSTRAINT payments_direction_valid CHECK (direction IN ('in', 'out')),
    -- A payment of nothing is not a payment. Refusing it here as well as in
    -- code, because it is a row-local fact.
    CONSTRAINT payments_amount_positive CHECK (amount > 0),
    CONSTRAINT payments_currency_format CHECK (currency_code ~ '^[A-Z]{3}$'),
    CONSTRAINT payments_reference_length CHECK (
        reference IS NULL OR char_length(reference) BETWEEN 1 AND 120
    ),
    CONSTRAINT payments_note_length CHECK (note IS NULL OR char_length(note) <= 2000),
    -- A draft has no number and everything else has one. The invoice's rule.
    CONSTRAINT payments_number_follows_status CHECK ((status = 'draft') = (number IS NULL)),
    CONSTRAINT payments_posted_when_numbered CHECK ((status = 'draft') = (posted_at IS NULL)),
    -- The conversion is all six columns or none of them. A rate with no date
    -- is a rate nobody can reproduce.
    CONSTRAINT payments_conversion_whole CHECK (
        (base_currency_code IS NULL
         AND exchange_rate IS NULL
         AND rate_date IS NULL
         AND base_amount IS NULL)
        OR (base_currency_code IS NOT NULL
            AND exchange_rate IS NOT NULL
            AND rate_date IS NOT NULL
            AND base_amount IS NOT NULL)
    )
);

CREATE UNIQUE INDEX payments_number_key ON payments (lower(number)) WHERE number IS NOT NULL;
-- The statement's query: everything one customer has paid, newest first.
CREATE INDEX payments_by_party ON payments (party_id, received_on DESC);
CREATE INDEX payments_by_date ON payments (received_on DESC);
CREATE INDEX payments_account ON payments (account_id);
-- Matching against a bank statement.
CREATE INDEX payments_reference ON payments (lower(reference)) WHERE reference IS NOT NULL;

CREATE TABLE payment_allocations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id  UUID NOT NULL REFERENCES payments (id) ON DELETE CASCADE,

    -- Books' own invoice, so a real foreign key. RESTRICT: an invoice somebody
    -- has paid against may not be deleted, and the invoice service already
    -- refuses to delete anything that is not a draft.
    invoice_id  UUID NOT NULL REFERENCES invoices (id) ON DELETE RESTRICT,

    -- In the PAYMENT's currency. A payment in euros against an invoice in
    -- euros is the ordinary case and the only one this allows: settling a
    -- dollar invoice with a euro cheque needs a realised-exchange-difference
    -- posting, which is a thing this ledger does not do yet, and a number
    -- stored here without it would be a silent loss.
    amount      NUMERIC(19, 4) NOT NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT payment_allocations_amount_positive CHECK (amount > 0)
);

-- One line per invoice per payment. Two rows would be one settlement recorded
-- twice, and the arithmetic that checks an invoice is not over-paid would pass
-- them both.
CREATE UNIQUE INDEX payment_allocations_once
    ON payment_allocations (payment_id, invoice_id);

-- "What has been paid against this invoice", which is the question every
-- outstanding figure on the statement asks.
CREATE INDEX payment_allocations_invoice ON payment_allocations (invoice_id);
