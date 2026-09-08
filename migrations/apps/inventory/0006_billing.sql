-- inventory 0006: the supplier's bill, completing the three-way match.
-- See ADR 0006 section 7 for the chain and section 6.5 for what this closes.

-- Added here rather than edited into 0005: that migration has run, so its
-- bytes are frozen. A quantity, not a flag, so partial billing subtracts.
ALTER TABLE receipt_lines
    ADD COLUMN billed NUMERIC(19, 6) NOT NULL DEFAULT 0;

ALTER TABLE receipt_lines
    ADD CONSTRAINT receipt_lines_billed_not_negative CHECK (billed >= 0);

CREATE TABLE bills (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    number      TEXT NOT NULL DEFAULT '',
    state       TEXT NOT NULL DEFAULT 'draft',

    order_id    UUID REFERENCES purchase_orders (id) ON DELETE RESTRICT,

    supplier_id   UUID NOT NULL,
    supplier_code TEXT NOT NULL,
    supplier_name TEXT NOT NULL,

    -- Their invoice number, and required: a bill nobody can tie to the
    -- supplier's own paper cannot be disputed or de-duplicated.
    supplier_reference TEXT NOT NULL,

    bill_date   DATE NOT NULL,
    due_on      DATE,

    currency    TEXT NOT NULL,
    net         NUMERIC(19, 4) NOT NULL DEFAULT 0,
    -- What the receipt accrued for these lines: the debit to GRNI.
    accrued     NUMERIC(19, 4) NOT NULL DEFAULT 0,
    -- net - accrued, posted to purchase price variance rather than absorbed
    -- into stock value. ADR 0006 section 6.1.
    variance    NUMERIC(19, 4) NOT NULL DEFAULT 0,

    note        TEXT,

    -- A force-post leaves its mark on the document, not only in the audit log.
    match_note  TEXT,
    overridden_by UUID REFERENCES core.users (id) ON DELETE SET NULL,
    overridden_at TIMESTAMPTZ,

    journal_id    UUID,
    journal_state TEXT NOT NULL DEFAULT 'pending',

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    posted_at   TIMESTAMPTZ,
    posted_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT bills_state_known CHECK (state IN ('draft', 'posted', 'cancelled')),
    CONSTRAINT bills_journal_state_known CHECK (
        journal_state IN ('pending', 'posted', 'no_ledger')
    ),
    CONSTRAINT bills_supplier_named CHECK (
        char_length(supplier_code) BETWEEN 1 AND 40
        AND char_length(supplier_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT bills_supplier_reference_present CHECK (
        char_length(supplier_reference) BETWEEN 1 AND 120
    ),
    CONSTRAINT bills_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT bills_numbered_when_posted CHECK ((state = 'posted') = (number <> '')),
    CONSTRAINT bills_note_length CHECK (note IS NULL OR char_length(note) <= 2000),
    CONSTRAINT bills_match_note_length CHECK (
        match_note IS NULL OR char_length(match_note) BETWEEN 1 AND 2000
    ),
    -- An override is a reason and a time together; a reason with no timestamp
    -- is an anonymous one.
    CONSTRAINT bills_override_whole CHECK (
        (match_note IS NULL AND overridden_by IS NULL AND overridden_at IS NULL)
        OR (match_note IS NOT NULL AND overridden_at IS NOT NULL)
    ),
    CONSTRAINT bills_due_after_dated CHECK (due_on IS NULL OR due_on >= bill_date)
);

CREATE UNIQUE INDEX bills_number ON bills (number) WHERE number <> '';

-- Stops the same invoice being keyed twice, which is where a duplicate payment
-- starts. Scoped to the supplier, and cancelled bills are excluded so a
-- mis-keyed bill can be cancelled and entered again.
CREATE UNIQUE INDEX bills_supplier_reference
    ON bills (supplier_id, lower(supplier_reference))
    WHERE state <> 'cancelled';

CREATE INDEX bills_supplier ON bills (supplier_id);
CREATE INDEX bills_order ON bills (order_id) WHERE order_id IS NOT NULL;
CREATE INDEX bills_date ON bills (bill_date DESC);
CREATE INDEX bills_due ON bills (due_on) WHERE state = 'posted' AND due_on IS NOT NULL;

COMMENT ON TABLE bills IS
    'Supplier invoices. Posting one clears goods-received-not-invoiced, posts any purchase price variance, and creates the payable.';

-- A line clears a RECEIPT line, since the receipt created the accrual; the
-- order line rides along because the agreed price lives there. Both are null
-- on a charge line - freight, a deposit, a rebate.
CREATE TABLE bill_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bill_id     UUID NOT NULL REFERENCES bills (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL,

    receipt_line_id UUID REFERENCES receipt_lines (id) ON DELETE RESTRICT,
    order_line_id   UUID REFERENCES purchase_order_lines (id) ON DELETE RESTRICT,

    variant_id  UUID REFERENCES item_variants (id) ON DELETE RESTRICT,
    description TEXT NOT NULL,

    quantity    NUMERIC(19, 6) NOT NULL,
    unit_id     UUID REFERENCES units (id) ON DELETE RESTRICT,

    unit_price  NUMERIC(19, 4) NOT NULL DEFAULT 0,
    net         NUMERIC(19, 4) NOT NULL DEFAULT 0,
    accrued     NUMERIC(19, 4) NOT NULL DEFAULT 0,

    CONSTRAINT bill_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT bill_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    ),
    CONSTRAINT bill_lines_goods_or_charge CHECK (
        (variant_id IS NULL AND unit_id IS NULL)
        OR (variant_id IS NOT NULL AND unit_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX bill_lines_position ON bill_lines (bill_id, line_no);
CREATE INDEX bill_lines_receipt_line ON bill_lines (receipt_line_id)
    WHERE receipt_line_id IS NOT NULL;
CREATE INDEX bill_lines_order_line ON bill_lines (order_line_id)
    WHERE order_line_id IS NOT NULL;
CREATE INDEX bill_lines_variant ON bill_lines (variant_id) WHERE variant_id IS NOT NULL;

-- The aged GRNI balance. Age is a column because a total nobody ages is how a
-- stale accrual sits until somebody writes the whole figure off.
CREATE VIEW unbilled_receipts AS
SELECT r.id                            AS receipt_id,
       r.number,
       r.received_on,
       r.supplier_id,
       r.supplier_name,
       o.number                        AS order_number,
       sum((l.quantity - l.billed) * l.unit_cost) AS unbilled,
       (CURRENT_DATE - r.received_on)  AS age_days
  FROM receipts r
  JOIN receipt_lines l ON l.receipt_id = r.id
  LEFT JOIN purchase_orders o ON o.id = r.order_id
 WHERE r.state = 'done'
   AND l.billed < l.quantity
 GROUP BY r.id, r.number, r.received_on, r.supplier_id, r.supplier_name, o.number
HAVING sum((l.quantity - l.billed) * l.unit_cost) <> 0;

COMMENT ON VIEW unbilled_receipts IS
    'Goods received and not yet billed, by receipt and by age. The aged half of the GRNI balance.';
