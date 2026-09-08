-- inventory 0005: the commitment, and the goods arriving against it.
--
-- WHAT THIS ADDS TO 0004
--
-- 0004 built the stock ledger: moves, quants, lots, layers. It is the half
-- everything moves through and the half nobody keys directly. This is the half
-- somebody has open when the lorry arrives.
--
--   purchase_orders       what was committed to, and to whom
--   purchase_order_lines  what, how many, at what price, and how much has come
--   receipts              goods arriving. The event with the accounting side
--   receipt_lines         what was on the pallet, and which move it became
--
-- WHY A RECEIPT AND NOT JUST A MOVE
--
-- A stock move is one item going from one place to another. A receipt is a
-- lorry: eight items, one delivery note, one supplier, one date, and one
-- journal. Without the document there is no row to hang the delivery note on,
-- nothing to reconcile a bill against, and nothing to print. The moves are
-- still where the truth lives, and `receipt_lines.move_id` is the thread from
-- the paperwork to the ledger.
--
-- THE NUMBERS ARE ALLOCATED AT CONFIRM AND AT POST
--
-- Not at create. A draft order somebody abandoned must not leave a hole in a
-- series a supplier and an auditor both read - ADR 0006 section 3, and the
-- rule `config/numbering/inventory.toml` already states for the adjustment.
-- This is a deliberate departure from Odoo, which numbers a quotation the
-- moment it is opened. Everything else here is Odoo's model.
--
-- WHAT IS DELIBERATELY ABSENT
--
-- No tax. A purchase order states quantities and prices; what tax is due is
-- decided by the BILL, against the group in force on the bill's own date. A
-- tax total here would be a number that disagrees with the bill the first day
-- a rate changes.
--
-- No foreign key to `master.parties`. `supplier_id` is a bare id with the code
-- and name snapshotted beside it, exactly as `books.invoices` carries one.
--
-- And no triggers. A posted receipt is evidence and is never edited, and that
-- rule lives in `phonix_db::inventory::receipt`: every statement that touches
-- one carries `WHERE state = 'draft'` in its own text. The CHECK constraints
-- below are row-local facts, which is what a constraint is for.

-- ---------------------------------------------------------------------------
-- Triggers 0004 left behind
-- ---------------------------------------------------------------------------
--
-- 0004 shipped with two triggers, and this codebase does not use them: a rule
-- enforced in a trigger is a rule nobody sees while writing the query it
-- governs, and one that cannot say anything useful when it fires. Both rules
-- now live in code - the append-only rule in
-- `phonix_db::inventory::movement`, where every statement carries its own
-- `WHERE state = 'draft'`, and the negative-stock floor in
-- `app_inventory::quant::take`, which can say how many units are short.
--
-- `IF EXISTS` throughout: a database provisioned after 0004 was corrected has
-- none of these, and a database that ran the earlier version has all of them.

DROP TRIGGER IF EXISTS stock_moves_append_only ON stock_moves;
DROP FUNCTION IF EXISTS stock_moves_are_append_only();

DROP TRIGGER IF EXISTS stock_quants_not_negative ON stock_quants;
DROP FUNCTION IF EXISTS stock_quants_do_not_go_negative();

-- ---------------------------------------------------------------------------
-- Purchase orders
-- ---------------------------------------------------------------------------

CREATE TABLE purchase_orders (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `PO-2026-00042`. Empty until confirmed; see the header.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | sent | confirmed | done | cancelled
    --
    -- No `received` and no `partially_received`. How much has arrived is
    -- arithmetic over the lines, and storing it as a state is a second fact
    -- about the same thing that stops agreeing the first time a receipt is
    -- cancelled. See `app_inventory::purchase::PurchaseOrder::receipt_state`.
    state       TEXT NOT NULL DEFAULT 'draft',

    -- A `master.parties` id with NO foreign key behind it. The two columns
    -- beside it are a snapshot taken at confirm: a supplier who renames
    -- themselves next year must not rewrite an order already sent.
    supplier_id     UUID NOT NULL,
    supplier_code   TEXT NOT NULL,
    supplier_name   TEXT NOT NULL,

    -- Where the goods are going. A receipt lands at this warehouse's Input or
    -- Stock location depending on how many steps it receives in.
    warehouse_id UUID NOT NULL REFERENCES warehouses (id) ON DELETE RESTRICT,

    order_date  DATE NOT NULL,
    expected_on DATE,

    -- ISO 4217, what the supplier quotes in. Converted to the workspace's own
    -- currency at the RECEIPT's date, because that is when the value arrives.
    currency    TEXT NOT NULL,

    -- Before tax, in `currency`. Stored rather than recomputed on read, on the
    -- same terms as an invoice's totals: the arithmetic is deterministic today
    -- and a rounding policy change would silently restate it next year.
    net         NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- An `hr.departments` id, resolved through the `CostCentres` port. No
    -- foreign key, for the same reason the supplier has none.
    cost_centre_id UUID,

    -- Their quotation number, off their paperwork.
    supplier_reference TEXT,
    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    confirmed_at TIMESTAMPTZ,
    confirmed_by UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT purchase_orders_state_known CHECK (
        state IN ('draft', 'sent', 'confirmed', 'done', 'cancelled')
    ),
    CONSTRAINT purchase_orders_supplier_named CHECK (
        char_length(supplier_code) BETWEEN 1 AND 40
        AND char_length(supplier_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT purchase_orders_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT purchase_orders_net_not_negative CHECK (net >= 0),
    -- A confirmed order has a number and a draft has none. The two facts are
    -- one fact, and this keeps them from drifting.
    CONSTRAINT purchase_orders_numbered_when_confirmed CHECK (
        (state IN ('confirmed', 'done')) = (number <> '')
    ),
    CONSTRAINT purchase_orders_expected_after_ordered CHECK (
        expected_on IS NULL OR expected_on >= order_date
    ),
    CONSTRAINT purchase_orders_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    )
);

-- Partial, so the many drafts with no number do not collide with each other.
CREATE UNIQUE INDEX purchase_orders_number ON purchase_orders (number) WHERE number <> '';
CREATE INDEX purchase_orders_supplier ON purchase_orders (supplier_id, order_date DESC);
CREATE INDEX purchase_orders_warehouse ON purchase_orders (warehouse_id);
CREATE INDEX purchase_orders_open ON purchase_orders (order_date DESC) WHERE state = 'confirmed';

CREATE TABLE purchase_order_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id    UUID NOT NULL REFERENCES purchase_orders (id) ON DELETE CASCADE,

    -- Position on the printed order, from one.
    line_no     INTEGER NOT NULL,

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,

    -- What the item was called when the order was raised. A printed order must
    -- not change its wording next year.
    description TEXT NOT NULL,

    -- How many, in the PURCHASE unit: cases, reels, whatever the supplier
    -- quotes in.
    quantity    NUMERIC(19, 6) NOT NULL,
    unit_id     UUID NOT NULL REFERENCES units (id) ON DELETE RESTRICT,

    -- The same quantity in the item's stock unit, converted ONCE when this row
    -- was written. Stored rather than converted on read: a conversion factor
    -- somebody edits next year would otherwise restate how much was ordered,
    -- and a receipt would be measured against a number the supplier never
    -- agreed to.
    quantity_stock NUMERIC(19, 6) NOT NULL,

    -- Per purchase unit, in the order's currency.
    unit_price  NUMERIC(19, 4) NOT NULL DEFAULT 0,
    net         NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- Running totals in STOCK units, advanced by each receipt and each bill.
    -- The two of them plus `quantity_stock` are the three-way match - ADR 0006
    -- section 6.5.
    received    NUMERIC(19, 6) NOT NULL DEFAULT 0,
    billed      NUMERIC(19, 6) NOT NULL DEFAULT 0,

    expected_on DATE,

    -- Struck out after confirming. Kept, because the order went out with it on.
    is_cancelled BOOLEAN NOT NULL DEFAULT FALSE,

    CONSTRAINT purchase_order_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT purchase_order_lines_stock_quantity_positive CHECK (quantity_stock > 0),
    CONSTRAINT purchase_order_lines_price_not_negative CHECK (unit_price >= 0),
    -- Received may exceed ordered - suppliers over-ship, and a system that
    -- refused to record it is asking a warehouse to write down a number it can
    -- see is wrong. It may not go below nothing.
    CONSTRAINT purchase_order_lines_received_not_negative CHECK (received >= 0),
    CONSTRAINT purchase_order_lines_billed_not_negative CHECK (billed >= 0),
    CONSTRAINT purchase_order_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    )
);

CREATE UNIQUE INDEX purchase_order_lines_position ON purchase_order_lines (order_id, line_no);
CREATE INDEX purchase_order_lines_variant ON purchase_order_lines (variant_id);
-- "What is still on order for this item", which is what a reordering rule and
-- a forecast both ask.
CREATE INDEX purchase_order_lines_outstanding
    ON purchase_order_lines (variant_id)
 WHERE NOT is_cancelled AND received < quantity_stock;

-- ---------------------------------------------------------------------------
-- Receipts
-- ---------------------------------------------------------------------------
--
-- WHERE THE GOODS LAND
--
-- `to_location_id` is the warehouse's Input location for a two- or three-step
-- warehouse and its Stock location for a one-step one. The put-away that
-- follows - Input to Quality Control to Stock - is an internal transfer, which
-- is a separate document for the reason Odoo makes it one: what has arrived and
-- what has been put away are different questions, and a warehouse that receives
-- in two steps is a warehouse that needs to tell them apart.

CREATE TABLE receipts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `IN-2026-00042`. Empty until posted.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | done | cancelled
    state       TEXT NOT NULL DEFAULT 'draft',

    -- The order this is against. NULL is ordinary: samples, customer returns
    -- and the first stock a workspace ever counts all arrive without one.
    order_id    UUID REFERENCES purchase_orders (id) ON DELETE RESTRICT,

    supplier_id   UUID NOT NULL,
    supplier_code TEXT NOT NULL,
    supplier_name TEXT NOT NULL,

    warehouse_id UUID NOT NULL REFERENCES warehouses (id) ON DELETE RESTRICT,
    to_location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,

    received_on DATE NOT NULL,

    -- Their delivery note number, typed off the paperwork. What somebody
    -- searches for when the supplier rings about a shortage.
    delivery_note TEXT,
    note        TEXT,

    -- What the goods were worth in the workspace's own currency: the figure
    -- the journal posted. Stored so a list can total without reading lines.
    value       NUMERIC(19, 4) NOT NULL DEFAULT 0,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    posted_at   TIMESTAMPTZ,
    posted_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT receipts_state_known CHECK (state IN ('draft', 'done', 'cancelled')),
    CONSTRAINT receipts_supplier_named CHECK (
        char_length(supplier_code) BETWEEN 1 AND 40
        AND char_length(supplier_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT receipts_numbered_when_posted CHECK ((state = 'done') = (number <> '')),
    CONSTRAINT receipts_note_length CHECK (note IS NULL OR char_length(note) <= 2000),
    CONSTRAINT receipts_delivery_note_length CHECK (
        delivery_note IS NULL OR char_length(delivery_note) BETWEEN 1 AND 120
    )
);

CREATE UNIQUE INDEX receipts_number ON receipts (number) WHERE number <> '';
CREATE INDEX receipts_order ON receipts (order_id) WHERE order_id IS NOT NULL;
CREATE INDEX receipts_supplier ON receipts (supplier_id, received_on DESC);
CREATE INDEX receipts_date ON receipts (received_on DESC);
-- The supplier rings about a shortage and quotes their own note number.
CREATE INDEX receipts_delivery_note ON receipts (lower(delivery_note))
    WHERE delivery_note IS NOT NULL;

CREATE TABLE receipt_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    receipt_id  UUID NOT NULL REFERENCES receipts (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL,

    -- Which order line this satisfies. NULL for a receipt with no order, and
    -- for an item that arrived which nobody ordered.
    order_line_id UUID REFERENCES purchase_order_lines (id) ON DELETE RESTRICT,

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    description TEXT NOT NULL,

    -- In the item's STOCK unit. Converted before it reaches this table, so
    -- nothing downstream of a receipt has to ask what unit a row is in.
    quantity    NUMERIC(19, 6) NOT NULL,

    -- The batch on the carton. Typed, never generated - it is the supplier's
    -- number and one this system invented would match nothing on the box. The
    -- `lots` row is created or found when the receipt is posted.
    lot_number  TEXT,
    expires_on  DATE,

    -- What one stock unit cost, in the workspace's own currency, at the rate on
    -- the receipt's date.
    unit_cost   NUMERIC(19, 4) NOT NULL DEFAULT 0,
    value       NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- The movement this line became, set at post. The thread from the
    -- paperwork to the stock ledger, and the reason a stock move can answer
    -- "which delivery note was this".
    move_id     UUID REFERENCES stock_moves (id) ON DELETE RESTRICT,

    CONSTRAINT receipt_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT receipt_lines_cost_not_negative CHECK (unit_cost >= 0),
    CONSTRAINT receipt_lines_lot_shape CHECK (
        lot_number IS NULL
        OR (char_length(lot_number) BETWEEN 1 AND 64 AND lot_number !~ '\s')
    ),
    CONSTRAINT receipt_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    )
);

CREATE UNIQUE INDEX receipt_lines_position ON receipt_lines (receipt_id, line_no);
CREATE INDEX receipt_lines_variant ON receipt_lines (variant_id);
CREATE INDEX receipt_lines_order_line ON receipt_lines (order_line_id)
    WHERE order_line_id IS NOT NULL;
-- One move belongs to one receipt line. A second row would be one delivery
-- counted twice.
CREATE UNIQUE INDEX receipt_lines_move ON receipt_lines (move_id) WHERE move_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- What is still on order
-- ---------------------------------------------------------------------------
--
-- The other half of "what have we got": on hand says what is on the shelf, and
-- this says what is coming. A reordering rule needs both, and a buyer looking
-- at an empty shelf needs to know an order for it went out on Tuesday before
-- raising a second one.

CREATE VIEW incoming_stock AS
SELECT l.variant_id,
       o.warehouse_id,
       sum(l.quantity_stock - l.received) AS quantity,
       min(COALESCE(l.expected_on, o.expected_on)) AS soonest_expected
  FROM purchase_order_lines l
  JOIN purchase_orders o ON o.id = l.order_id
 WHERE o.state = 'confirmed'
   AND NOT l.is_cancelled
   AND l.received < l.quantity_stock
 GROUP BY l.variant_id, o.warehouse_id;

COMMENT ON VIEW incoming_stock IS
    'What is on order and not yet arrived, per variant and warehouse. The other half of what is on hand.';
