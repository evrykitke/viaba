-- inventory 0009: landed cost, and the value a supplier's invoice leaves out.
--
-- WHAT THIS FIXES
--
-- ADR 0006 section 6.2. Freight, duty, insurance and handling are expensed when
-- the carrier's invoice arrives rather than capitalised into the value of the
-- goods they carried. Inventory is then carried below what it cost, every
-- margin on every sale of those goods is overstated, and the error is invisible
-- because both halves are individually correct - the freight bill is a real
-- expense and the receipt was valued at the price on the supplier's invoice.
--
-- WHAT A LANDED COST IS NOT
--
-- It is not the freight bill. The carrier's invoice is an ordinary bill coded
-- to a freight account, keyed and paid like any other. THIS document is the
-- allocation: it names a receipt, names the charges to spread over it, and
-- moves that money out of expense and into the value of what is on the shelf.
--
-- Two documents rather than one because the two facts arrive separately and
-- from different people. The goods land in the warehouse on Tuesday; the
-- carrier invoices in three weeks; the customs agent in six. A model that
-- required them together would be a model in which nothing is ever capitalised.
--
--   landed_costs              the allocation. One receipt, one date, one number
--   landed_cost_charges       what is being spread, and on what basis
--   landed_cost_allocations   what each charge did to each received line
--
-- WHY THE BASIS IS ON THE CHARGE AND NOT ON THE DOCUMENT
--
-- ADR 0006 section 6.2 says the basis is stored on the document, and it is -
-- a charge row IS the document. It is one column further down than the ADR's
-- sentence implies, and deliberately: freight is allocated by weight, insurance
-- and duty by value, and a customs clearance fee by the number of cartons.
-- Forcing one basis on a document holding all three would make two of the
-- three wrong, and "why is this unit 4.12 and that one 4.09" is answerable a
-- year later only if each charge kept the basis it was actually spread on.
--
-- WHY AN ALLOCATION ROW EXISTS AT ALL
--
-- It is derivable at the moment of posting and never again: the basis it was
-- computed from is the receipt as it stood, and the receipt's own value moves
-- when a bill posts a price difference against it. Recomputing later answers a
-- different question than the one that was posted. The row is the arithmetic,
-- kept.
--
-- NO TRIGGERS, per ADR 0006. `WHERE state = 'draft'` is in the text of every
-- statement that changes one of these rows, where a reader can see it.

-- ---------------------------------------------------------------------------
-- What a layer gained after the fact
-- ---------------------------------------------------------------------------
--
-- A layer's `unit_cost` and `value` are what the SUPPLIER charged, and they
-- stay that. The freight is a second number beside them, not an edit of the
-- first, because the two are different facts and the question a year later -
-- "was this unit dear because we paid too much or because it came by air" - has
-- an answer only while they are apart.
--
-- What stock is worth is therefore `value + additional_value`, and a unit of it
-- is that over `quantity`. Every reader of a layer's cost has to use the sum;
-- `phonix_db::inventory::valuation` is the one place that reads them.
--
-- It may be negative - a carrier credits an overcharge, and the correction is a
-- landed cost with a negative charge on it, which is the reversing entry rule
-- section 6.3 already applies to everything else. What it may not do is take
-- the layer below nothing, and that is a row-local fact, so it is a CHECK.

ALTER TABLE valuation_layers
    ADD COLUMN additional_value NUMERIC(19, 4) NOT NULL DEFAULT 0;

ALTER TABLE valuation_layers
    ADD CONSTRAINT valuation_layers_landed_value_not_negative
    CHECK (value + additional_value >= 0);

COMMENT ON COLUMN valuation_layers.additional_value IS
    'Landed cost added to this layer after the receipt. Stock is worth value + additional_value.';

-- ---------------------------------------------------------------------------
-- The document
-- ---------------------------------------------------------------------------

CREATE TABLE landed_costs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `LC-2026-00007`. Empty until posted, on the same terms as every other
    -- document in this schema: a draft somebody abandoned must not leave a hole
    -- in a series an auditor reads.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | done | cancelled
    state       TEXT NOT NULL DEFAULT 'draft',

    -- The delivery this is being spread over. RESTRICT: the allocation is
    -- meaningless without the lines it was computed against, and a receipt that
    -- has been landed on is not one anybody may delete.
    --
    -- ONE receipt, not several. A carrier's invoice covering three deliveries is
    -- three landed costs with a third of the charge on each, and the split is a
    -- decision somebody makes rather than one this system guesses. The
    -- alternative - a document spanning receipts - has to invent a basis for
    -- apportioning between them before it can apportion within them.
    receipt_id  UUID NOT NULL REFERENCES receipts (id) ON DELETE RESTRICT,

    -- Snapshots, so a list draws without joining. Refreshed never: they are
    -- what the receipt was called when this was written.
    receipt_number TEXT NOT NULL DEFAULT '',
    supplier_name  TEXT NOT NULL,

    -- The date the journal takes. Not the receipt's: the freight was incurred
    -- when the carrier invoiced, and posting it into a closed period is what
    -- the period lock is for.
    cost_date   DATE NOT NULL,

    note        TEXT,

    -- Sums of the charge lines and of what the allocation did with them, in the
    -- workspace's base currency. Stored so a list totals without reading lines,
    -- and so the two halves can be seen not to have drifted.
    total       NUMERIC(19, 4) NOT NULL DEFAULT 0,
    -- What went into the value of stock still on the shelf.
    capitalised NUMERIC(19, 4) NOT NULL DEFAULT 0,
    -- What belonged to units already issued, and went to cost of sales instead.
    expensed    NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- not_required | no_ledger | posted. The three states a stock move takes,
    -- for the same reason and with the same meaning - see 0004's header.
    journal_state  TEXT NOT NULL DEFAULT 'not_required',
    journal_id     UUID,
    journal_number TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    posted_at   TIMESTAMPTZ,
    posted_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT landed_costs_state_known CHECK (state IN ('draft', 'done', 'cancelled')),
    CONSTRAINT landed_costs_numbered_when_posted CHECK ((state = 'done') = (number <> '')),
    CONSTRAINT landed_costs_supplier_named CHECK (
        char_length(supplier_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT landed_costs_note_length CHECK (note IS NULL OR char_length(note) <= 2000),
    -- The two halves of what was allocated are the whole of it. This is the one
    -- arithmetic fact about the document that is row-local, and it is the one
    -- that would be silently wrong if the allocation ever lost a penny.
    CONSTRAINT landed_costs_allocation_is_whole CHECK (
        state <> 'done' OR capitalised + expensed = total
    ),
    CONSTRAINT landed_costs_journal_state_known CHECK (
        journal_state IN ('not_required', 'no_ledger', 'posted')
    ),
    CONSTRAINT landed_costs_journal_is_whole CHECK (
        (journal_state = 'posted') = (journal_id IS NOT NULL)
        AND (journal_id IS NULL) = (journal_number IS NULL)
    )
);

CREATE UNIQUE INDEX landed_costs_number ON landed_costs (number) WHERE number <> '';
CREATE INDEX landed_costs_receipt ON landed_costs (receipt_id);
CREATE INDEX landed_costs_date ON landed_costs (cost_date DESC);
CREATE INDEX landed_costs_open ON landed_costs (cost_date DESC) WHERE state = 'draft';
CREATE INDEX landed_costs_journal ON landed_costs (journal_id) WHERE journal_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- What is being spread
-- ---------------------------------------------------------------------------

CREATE TABLE landed_cost_charges (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    landed_cost_id UUID NOT NULL REFERENCES landed_costs (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL,

    -- "Sea freight, Felixstowe", "Import duty", "Customs clearance". Typed:
    -- these are what the carrier called them, and a picklist would be a list
    -- somebody has to maintain before they can key an invoice.
    description TEXT NOT NULL,

    -- value | quantity | weight. See the file header for why it is here rather
    -- than on the document.
    basis       TEXT NOT NULL,

    -- In the workspace's base currency. NEGATIVE IS ALLOWED and is the whole of
    -- how a landed cost is corrected: the carrier credits an overcharge, and
    -- the credit is a second document with a negative charge on it. Zero is
    -- not - a line that spreads nothing is a line somebody meant to delete.
    amount      NUMERIC(19, 4) NOT NULL,

    CONSTRAINT landed_cost_charges_basis_known CHECK (
        basis IN ('value', 'quantity', 'weight')
    ),
    CONSTRAINT landed_cost_charges_amount_not_zero CHECK (amount <> 0),
    CONSTRAINT landed_cost_charges_description_length CHECK (
        char_length(description) BETWEEN 1 AND 200
    )
);

CREATE UNIQUE INDEX landed_cost_charges_position
    ON landed_cost_charges (landed_cost_id, line_no);

-- ---------------------------------------------------------------------------
-- The arithmetic, kept
-- ---------------------------------------------------------------------------
--
-- One row per charge per received line. Written at post and never again, and
-- it is what answers "why is this unit 4.12" - the basis, what this line
-- contributed to it, and what fell out.
--
-- CAPITALISED AND EXPENSED
--
-- Freight arriving six weeks after the goods is freight on stock that has
-- partly been sold. The share belonging to units still in the layer goes into
-- the layer; the share belonging to units already issued cannot - those units
-- are gone and their cost was taken to cost of sales at a figure that was too
-- low. It goes to cost of sales too, which is where it would have gone had the
-- carrier invoiced on time.
--
-- Splitting it is not a nicety. Putting all of it into the layer would value
-- the remaining units at the freight for units that are not there, which is the
-- same class of error section 6.2 is about, arrived at from the other side.

CREATE TABLE landed_cost_allocations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    landed_cost_id UUID NOT NULL REFERENCES landed_costs (id) ON DELETE CASCADE,
    charge_id   UUID NOT NULL REFERENCES landed_cost_charges (id) ON DELETE CASCADE,

    receipt_line_id UUID NOT NULL REFERENCES receipt_lines (id) ON DELETE RESTRICT,
    -- The layer this landed on. NOT NULL: a received line with no layer is not
    -- carrying value, and lines like that are dropped from the basis rather
    -- than allocated a share they cannot hold.
    layer_id    UUID NOT NULL REFERENCES valuation_layers (id) ON DELETE RESTRICT,
    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,

    -- Snapshot of the charge's basis, so this row reads on its own.
    basis       TEXT NOT NULL,
    -- What this line contributed to the basis: its value, its quantity, or its
    -- weight in grams. Six decimal places because a quantity has six; a value
    -- basis fits inside that with room to spare.
    basis_amount NUMERIC(19, 6) NOT NULL,

    -- This line's share of the charge, and the two halves it broke into.
    amount      NUMERIC(19, 4) NOT NULL,
    capitalised NUMERIC(19, 4) NOT NULL,
    expensed    NUMERIC(19, 4) NOT NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT landed_cost_allocations_basis_known CHECK (
        basis IN ('value', 'quantity', 'weight')
    ),
    -- The share is the two halves. Row-local, and the reason a penny cannot go
    -- missing between the layer and the ledger.
    CONSTRAINT landed_cost_allocations_split_is_whole CHECK (
        capitalised + expensed = amount
    ),

    -- One charge reaches one line once. A second row is the same freight
    -- capitalised twice.
    UNIQUE (charge_id, receipt_line_id)
);

CREATE INDEX landed_cost_allocations_document ON landed_cost_allocations (landed_cost_id);
CREATE INDEX landed_cost_allocations_layer ON landed_cost_allocations (layer_id);
CREATE INDEX landed_cost_allocations_receipt_line
    ON landed_cost_allocations (receipt_line_id);

-- ---------------------------------------------------------------------------
-- What a receipt has been landed with
-- ---------------------------------------------------------------------------
--
-- The question the receipt screen asks: has anything been added to this
-- delivery since it was posted, and what. A receipt with no row here has had
-- nothing landed on it, which is the ordinary case and the reason this is a
-- view rather than a column somebody has to keep in step.

CREATE VIEW receipt_landed_cost AS
SELECT c.receipt_id,
       count(*)                  AS document_count,
       sum(c.total)              AS total,
       sum(c.capitalised)        AS capitalised,
       sum(c.expensed)           AS expensed,
       max(c.cost_date)          AS latest_on
  FROM landed_costs c
 WHERE c.state = 'done'
 GROUP BY c.receipt_id;

COMMENT ON VIEW receipt_landed_cost IS
    'What has been capitalised onto each receipt since it was posted. No row means nothing was.';
