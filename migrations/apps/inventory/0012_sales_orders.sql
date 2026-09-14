-- inventory 0012: the commitment on the way out.
--
-- 0005 built the buying chain: the order, the goods arriving, the bill. This is
-- its mirror, and it is the half this workspace has never had. Books could
-- raise an invoice and Inventory could move stock to a customer, and nothing
-- anywhere said what had been AGREED - so "what have we promised to ship" had
-- no answer, and every delivery was somebody remembering.
--
--   sales_orders        what was agreed, and with whom
--   sales_order_lines   what, how many, at what price, and how much has gone
--
-- ONE RECORD, NOT TWO
--
-- A quotation and a sales order are the same document in two states, which is
-- Odoo's model and the one this codebase already follows for the purchase
-- order. ERPNext makes them two doctypes and then has to copy one into the
-- other; the copy is where the two stop agreeing.
--
-- THE NUMBER IS ALLOCATED WHEN IT LEAVES THE BUILDING
--
-- Not at create, and - unlike the purchase order - not at confirm either. A
-- quotation is SENT to a customer, who then quotes it back on their purchase
-- order and on the phone; a quotation with no number is one nobody can cite.
-- So the number is taken at the first move out of `draft`, whether that is
-- `sent` or a confirmation straight from a draft, which is what an order taken
-- over the counter is.
--
-- A quotation that is never accepted KEEPS its number, exactly as a rejected
-- requisition does and for the same reason: somebody was quoted it.
--
-- WHAT IS DELIBERATELY ABSENT
--
-- No tax, on the same terms as the purchase order: an order states quantities
-- and prices, and what tax is due is decided by the INVOICE against the group
-- in force on the invoice's own date. A tax total here would disagree with the
-- invoice the first day a rate changes.
--
-- No foreign key to `master.parties`, and none to `books.invoices` either. The
-- customer is a bare id with the code and name snapshotted beside it, and what
-- has been invoiced is a quantity on the line rather than a link to a document
-- in another app's schema - ADR 0001.

CREATE TABLE sales_orders (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `SO-2026-00042`. Empty only while it is a draft; see the header.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | sent | confirmed | done | cancelled
    --
    -- No `delivered` and no `partially_delivered`. How much has gone is
    -- arithmetic over the lines, and storing it as a state is a second fact
    -- about the same thing that stops agreeing the first time a delivery is
    -- cancelled. Same argument as `purchase_orders.state`.
    state       TEXT NOT NULL DEFAULT 'draft',

    -- A `master.parties` id with NO foreign key behind it, snapshotted at the
    -- moment the document leaves: a customer who renames themselves next year
    -- must not rewrite a quotation already sent.
    customer_id   UUID NOT NULL,
    customer_code TEXT NOT NULL,
    customer_name TEXT NOT NULL,

    -- Which warehouse ships it. A delivery leaves this warehouse's Output or
    -- Stock location depending on how many steps it delivers in - the mirror
    -- of a receipt landing at Input or Stock.
    warehouse_id UUID NOT NULL REFERENCES warehouses (id) ON DELETE RESTRICT,

    order_date  DATE NOT NULL,

    -- What was promised. The date a despatch list is sorted by, and the one a
    -- customer quotes back when it slips.
    promised_on DATE,

    -- How long the quotation stands. Only meaningful before confirmation, and
    -- kept afterwards because what was offered is part of what was agreed.
    valid_until DATE,

    -- ISO 4217, what the customer is quoted in. Converted to the workspace's
    -- own currency by the INVOICE, at the invoice's date.
    currency    TEXT NOT NULL,

    -- Before tax, in `currency`. Stored rather than recomputed on read, on the
    -- same terms as an invoice's totals.
    net         NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- Their purchase order number, off their paperwork. What somebody searches
    -- for when the customer rings.
    customer_reference TEXT,
    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    issued_at   TIMESTAMPTZ,
    issued_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,
    confirmed_at TIMESTAMPTZ,
    confirmed_by UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT sales_orders_state_known CHECK (
        state IN ('draft', 'sent', 'confirmed', 'done', 'cancelled')
    ),
    CONSTRAINT sales_orders_customer_named CHECK (
        char_length(customer_code) BETWEEN 1 AND 40
        AND char_length(customer_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT sales_orders_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT sales_orders_net_not_negative CHECK (net >= 0),

    -- A draft has no number and anything that has been sent has one. Cancelled
    -- is left open on purpose: a quotation cancelled before it was ever sent
    -- never took a number, and one cancelled afterwards keeps the number it
    -- was quoted under. Both are true, and a constraint that demanded one of
    -- them would be wrong half the time.
    CONSTRAINT sales_orders_numbered_once_issued CHECK (
        (state = 'draft' AND number = '')
        OR (state IN ('sent', 'confirmed', 'done') AND number <> '')
        OR state = 'cancelled'
    ),
    CONSTRAINT sales_orders_promised_after_ordered CHECK (
        promised_on IS NULL OR promised_on >= order_date
    ),
    CONSTRAINT sales_orders_valid_after_ordered CHECK (
        valid_until IS NULL OR valid_until >= order_date
    ),
    CONSTRAINT sales_orders_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    ),
    CONSTRAINT sales_orders_reference_length CHECK (
        customer_reference IS NULL OR char_length(customer_reference) BETWEEN 1 AND 120
    )
);

-- Partial, so the many drafts with no number do not collide with each other.
CREATE UNIQUE INDEX sales_orders_number ON sales_orders (number) WHERE number <> '';
CREATE INDEX sales_orders_customer ON sales_orders (customer_id, order_date DESC);
CREATE INDEX sales_orders_warehouse ON sales_orders (warehouse_id);
-- The despatch list: what is agreed and still owed, soonest first.
CREATE INDEX sales_orders_open ON sales_orders (promised_on) WHERE state = 'confirmed';
-- The customer rings and quotes their own order number.
CREATE INDEX sales_orders_customer_reference ON sales_orders (lower(customer_reference))
    WHERE customer_reference IS NOT NULL;

CREATE TABLE sales_order_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id    UUID NOT NULL REFERENCES sales_orders (id) ON DELETE CASCADE,

    -- Position on the printed order, from one.
    line_no     INTEGER NOT NULL,

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,

    -- What the item was called when the order was raised. A quotation already
    -- sent must not change its wording next year.
    description TEXT NOT NULL,

    -- How many, in the unit the customer is quoted in.
    quantity    NUMERIC(19, 6) NOT NULL,
    unit_id     UUID NOT NULL REFERENCES units (id) ON DELETE RESTRICT,

    -- The same quantity in the item's stock unit, converted ONCE when this row
    -- was written - for the reason `purchase_order_lines.quantity_stock` gives:
    -- a conversion factor somebody edits next year must not restate what was
    -- agreed.
    quantity_stock NUMERIC(19, 6) NOT NULL,

    -- Per quoted unit, in the order's currency.
    unit_price  NUMERIC(19, 4) NOT NULL DEFAULT 0,
    net         NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- Running totals in STOCK units, advanced by each delivery and each
    -- invoice. The pair is what makes "ordered but not shipped" and "shipped
    -- but not billed" arithmetic rather than an investigation - and the second
    -- is the sell side of ADR 0006 section 6.5.
    delivered   NUMERIC(19, 6) NOT NULL DEFAULT 0,
    invoiced    NUMERIC(19, 6) NOT NULL DEFAULT 0,

    promised_on DATE,

    -- Struck out after confirming. Kept, because the order was agreed with it
    -- on and the customer has a copy.
    is_cancelled BOOLEAN NOT NULL DEFAULT FALSE,

    CONSTRAINT sales_order_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT sales_order_lines_stock_quantity_positive CHECK (quantity_stock > 0),
    CONSTRAINT sales_order_lines_price_not_negative CHECK (unit_price >= 0),
    -- Over-delivery is allowed to be recorded for the reason over-receipt is:
    -- it happens, and a system that refuses to write down what left the
    -- building is asking a warehouse to lie.
    CONSTRAINT sales_order_lines_delivered_not_negative CHECK (delivered >= 0),
    CONSTRAINT sales_order_lines_invoiced_not_negative CHECK (invoiced >= 0),
    CONSTRAINT sales_order_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    )
);

CREATE UNIQUE INDEX sales_order_lines_position ON sales_order_lines (order_id, line_no);
CREATE INDEX sales_order_lines_variant ON sales_order_lines (variant_id);
-- "What is promised for this item", which is what an availability check and a
-- despatch list both ask.
CREATE INDEX sales_order_lines_outstanding
    ON sales_order_lines (variant_id)
 WHERE NOT is_cancelled AND delivered < quantity_stock;

-- ---------------------------------------------------------------------------
-- What is promised and not yet gone
-- ---------------------------------------------------------------------------
--
-- The mirror of `incoming_stock`. On hand says what is on the shelf; that view
-- says what is coming; this says what is already spoken for. A salesperson
-- looking at forty on hand needs to know thirty-five of them are promised
-- before agreeing to ship thirty.

CREATE VIEW committed_stock AS
SELECT l.variant_id,
       o.warehouse_id,
       sum(l.quantity_stock - l.delivered) AS quantity,
       min(COALESCE(l.promised_on, o.promised_on)) AS soonest_promised
  FROM sales_order_lines l
  JOIN sales_orders o ON o.id = l.order_id
 WHERE o.state = 'confirmed'
   AND NOT l.is_cancelled
   AND l.delivered < l.quantity_stock
 GROUP BY l.variant_id, o.warehouse_id;

COMMENT ON VIEW committed_stock IS
    'What is agreed and not yet shipped, per variant and warehouse. The mirror of incoming_stock.';
