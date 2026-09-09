-- inventory 0008: eleven departments wanting printer paper, bought once.
--
-- WHAT THIS ADDS
--
--   consolidations             a buyer gathering approved demand
--   consolidation_lines        one item, one supplier, one quantity to buy
--   purchase_order_line_sources  which requisition lines an order line satisfied
--
-- and one column 0007 should have had.
--
-- WHY THIS IS A DOCUMENT AND NOT A BUTTON
--
-- Grouping demand by item is supplier-agnostic; deciding who to buy each item
-- from is not. Between those two facts sits a decision somebody makes - and one
-- consolidation routinely becomes SEVERAL purchase orders, because no single
-- supplier stocks everything eleven departments asked for. A screen that
-- produced one order would either force one supplier or silently drop the rest.
--
-- This is the failure the Dynamics 365 literature describes: consolidation that
-- merges across vendors and delivery addresses, leaving a buyer to rebuild the
-- request by hand late in the process, and a receiving end that cannot split
-- what arrives. So the supplier is chosen per LINE, and confirming splits the
-- document into one order per supplier.
--
-- The warehouse is on the header rather than the line, for the other half of
-- that same failure: demand for two buildings consolidated into one order is an
-- order that cannot be received in either.
--
-- WHAT `purchase_order_line_sources` IS FOR
--
-- ADR 0006 section 7: "every line of that order remembers which requisitions it
-- came from - because when it arrives, the cost has to be split back across the
-- cost centres that asked for it, and a consolidation that forgets its inputs
-- cannot do that."
--
-- This is that table. It is written at CONFIRM, not at draft, and the
-- allocation is recomputed then rather than remembered from when the line was
-- drawn: a consolidation drafted last week and confirmed today must allocate
-- against the demand that is outstanding *now*, or it orders things somebody
-- has since withdrawn.
--
-- WHAT IT DELIBERATELY DOES NOT DO
--
-- It does not force the sources to add up to the order line. A buyer rounding
-- 47 reams up to a case of 50 is the ordinary case, and the three extra reams
-- belong to no requisition: they are stock. Most systems either refuse the
-- rounding or quietly attach the excess to the last requisition, which charges
-- a department for something it did not ask for. Here the difference is visible
-- - see `purchase_order_line_allocation` at the foot - and charged to nobody.

-- ---------------------------------------------------------------------------
-- The column 0007 should have had
-- ---------------------------------------------------------------------------
--
-- Added here rather than edited into 0007, which is published: its bytes are
-- frozen, and 0006 set this precedent against 0005 for the same reason.
--
-- A requisition line is placed in the unit the requester thinks in - "two boxes
-- of gloves" - and `requisition_demand` sums those lines across requesters.
-- Without a common unit that sum adds three boxes to two eaches and reports
-- five of nothing. `purchase_order_lines` has carried `quantity_stock` since
-- 0005 for exactly this reason; the requisition needs it for the same one.
--
-- Converted ONCE when the line is written, never on read: a conversion factor
-- somebody edits next year must not restate how much a department asked for.

ALTER TABLE requisition_lines
    ADD COLUMN quantity_stock NUMERIC(19, 6) NOT NULL DEFAULT 0;

-- Every existing row, if any workspace has some. The requisition form has only
-- ever offered the item's own units, so quantity is the best available answer
-- and is exact wherever the requester used the stock unit.
UPDATE requisition_lines SET quantity_stock = quantity WHERE quantity_stock = 0;

-- The default was for the backfill. A row written from here on states it.
ALTER TABLE requisition_lines ALTER COLUMN quantity_stock DROP DEFAULT;

ALTER TABLE requisition_lines
    ADD CONSTRAINT requisition_lines_quantity_stock_positive CHECK (quantity_stock > 0);

-- `ordered` is in stock units too, and always was - it is advanced by an order
-- line, which is measured that way. Said out loud because the column sits
-- beside `quantity`, which is not.
COMMENT ON COLUMN requisition_lines.ordered IS
    'How much of this line has reached a purchase order, in the item''s stock unit.';
COMMENT ON COLUMN requisition_lines.quantity IS
    'How many, in the unit the requester asked in.';
COMMENT ON COLUMN requisition_lines.quantity_stock IS
    'The same quantity in the item''s stock unit, converted once when the line was written. What demand is summed in.';

-- The demand view has to sum a common unit, which is the whole point above.
DROP VIEW requisition_demand;

CREATE VIEW requisition_demand AS
SELECT l.variant_id,
       r.warehouse_id,
       count(*)                             AS lines,
       count(DISTINCT r.id)                 AS requisitions,
       sum(l.quantity_stock - l.ordered)    AS outstanding,
       min(r.needed_by)                     AS needed_by,
       min(r.raised_on)                     AS oldest_request
  FROM requisition_lines l
  JOIN requisitions r ON r.id = l.requisition_id
 WHERE r.state = 'approved'
   AND l.ordered < l.quantity_stock
 GROUP BY l.variant_id, r.warehouse_id;

COMMENT ON VIEW requisition_demand IS
    'Approved requisition lines with something still to order, grouped by what and where, in stock units. What consolidation turns into purchase orders.';

-- And the constraint 0007 wrote against the wrong column.
ALTER TABLE requisition_lines
    DROP CONSTRAINT requisition_lines_ordered_within_request;

ALTER TABLE requisition_lines
    ADD CONSTRAINT requisition_lines_ordered_within_request
    CHECK (ordered <= quantity_stock);

-- ---------------------------------------------------------------------------
-- Consolidations
-- ---------------------------------------------------------------------------

CREATE TABLE consolidations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `CON-2026-00042`. Empty until confirmed, on the same terms as every other
    -- document here: a draft somebody abandoned must not leave a hole.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | confirmed | cancelled
    --
    -- There is no `ordered` state. Confirming IS the ordering, and what came of
    -- it is the orders themselves - which name this document, so "what did this
    -- consolidation become" is a query rather than a column.
    state       TEXT NOT NULL DEFAULT 'draft',

    -- One warehouse per consolidation - see the header.
    warehouse_id UUID NOT NULL REFERENCES warehouses (id) ON DELETE RESTRICT,

    raised_on   DATE NOT NULL,
    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    confirmed_at TIMESTAMPTZ,
    confirmed_by UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT consolidations_state_known CHECK (
        state IN ('draft', 'confirmed', 'cancelled')
    ),
    CONSTRAINT consolidations_numbered_when_confirmed CHECK (
        (state = 'confirmed') = (number <> '')
    ),
    CONSTRAINT consolidations_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    )
);

CREATE UNIQUE INDEX consolidations_number ON consolidations (number) WHERE number <> '';
CREATE INDEX consolidations_raised ON consolidations (raised_on DESC);
CREATE INDEX consolidations_warehouse ON consolidations (warehouse_id);
CREATE INDEX consolidations_open ON consolidations (raised_on DESC) WHERE state = 'draft';

COMMENT ON TABLE consolidations IS
    'A buyer gathering approved requisition demand for one warehouse. Confirming it raises one purchase order per supplier.';

-- Which consolidation raised an order, said on the order itself.
--
-- `purchase_order_line_sources` nearly answers this by join, but not quite: a
-- consolidation whose every line was bought for stock writes no source rows at
-- all, and the orders it raised would then belong to nothing. The link is a
-- fact about the order, so it lives on the order.
--
-- SET NULL rather than CASCADE: deleting a consolidation must never take a
-- purchase order with it. The schema only lets a draft be deleted, and a draft
-- has raised none - but an order is a commitment somebody made to a supplier,
-- and no cleanup anywhere should be able to reach it.

ALTER TABLE purchase_orders
    ADD COLUMN consolidation_id UUID REFERENCES consolidations (id) ON DELETE SET NULL;

CREATE INDEX purchase_orders_consolidation ON purchase_orders (consolidation_id)
    WHERE consolidation_id IS NOT NULL;

COMMENT ON COLUMN purchase_orders.consolidation_id IS
    'The consolidation this order was raised by, where one was. NULL for an order a buyer wrote directly.';

CREATE TABLE consolidation_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    consolidation_id UUID NOT NULL REFERENCES consolidations (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL,

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    description TEXT NOT NULL,

    -- How much to buy, in the item's STOCK unit. Editable, and deliberately not
    -- pinned to `demand` below: rounding up to a case is the ordinary case.
    quantity    NUMERIC(19, 6) NOT NULL,

    -- What was outstanding when this line was drawn. Kept so the screen can say
    -- how much of the order is demand and how much is the buyer's own decision,
    -- and so a stale draft is visible as one. It is NOT what gets allocated -
    -- that is recomputed at confirm.
    demand      NUMERIC(19, 6) NOT NULL,

    -- Who to buy it from. Per line, so one document becomes several orders.
    -- A `master.parties` id with no foreign key, snapshotted beside, exactly as
    -- a purchase order carries one.
    supplier_id   UUID,
    supplier_code TEXT,
    supplier_name TEXT,

    -- What the buyer expects to pay, per stock unit, in `currency`. Carried on
    -- to the order line it becomes; blank takes the item's own cost, which is
    -- what the order form does with a blank price.
    unit_price  NUMERIC(19, 4),
    currency    TEXT,

    note        TEXT,

    CONSTRAINT consolidation_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT consolidation_lines_demand_not_negative CHECK (demand >= 0),
    CONSTRAINT consolidation_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    ),
    -- The three supplier columns are one snapshot; two of them filled is a
    -- supplier half-remembered.
    CONSTRAINT consolidation_lines_supplier_whole CHECK (
        (supplier_id IS NULL AND supplier_code IS NULL AND supplier_name IS NULL)
        OR (supplier_id IS NOT NULL
            AND char_length(supplier_code) BETWEEN 1 AND 40
            AND char_length(supplier_name) BETWEEN 1 AND 200)
    ),
    -- A price means nothing without the currency it is in.
    CONSTRAINT consolidation_lines_priced_in_something CHECK (
        unit_price IS NULL OR currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT consolidation_lines_price_not_negative CHECK (
        unit_price IS NULL OR unit_price >= 0
    ),
    CONSTRAINT consolidation_lines_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    )
);

CREATE UNIQUE INDEX consolidation_lines_position
    ON consolidation_lines (consolidation_id, line_no);
-- One item once per document. Two lines for the same thing is the keying
-- mistake that produces two orders to two suppliers for one requirement.
CREATE UNIQUE INDEX consolidation_lines_item
    ON consolidation_lines (consolidation_id, variant_id);
CREATE INDEX consolidation_lines_supplier ON consolidation_lines (supplier_id)
    WHERE supplier_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- What an order line was ordered for
-- ---------------------------------------------------------------------------
--
-- Written at confirm, one row per requisition line an order line satisfies, in
-- stock units. RESTRICT on the requisition line: a request that has been bought
-- against is evidence, and deleting it would leave an order nobody can explain.
--
-- No cost-centre snapshot here, unlike most rows in this schema. The
-- requisition already holds one and cannot be edited after submit, so the join
-- is always available and a second copy could only ever disagree.

CREATE TABLE purchase_order_line_sources (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_line_id UUID NOT NULL REFERENCES purchase_order_lines (id) ON DELETE CASCADE,
    requisition_line_id UUID NOT NULL REFERENCES requisition_lines (id) ON DELETE RESTRICT,

    -- The consolidation this allocation was made by, for the audit trail.
    -- NULL where an order was raised without one.
    consolidation_id UUID REFERENCES consolidations (id) ON DELETE SET NULL,

    quantity    NUMERIC(19, 6) NOT NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT purchase_order_line_sources_quantity_positive CHECK (quantity > 0)
);

-- One allocation per pair. A second row would be the same demand counted twice.
CREATE UNIQUE INDEX purchase_order_line_sources_pair
    ON purchase_order_line_sources (order_line_id, requisition_line_id);
CREATE INDEX purchase_order_line_sources_requisition_line
    ON purchase_order_line_sources (requisition_line_id);
CREATE INDEX purchase_order_line_sources_consolidation
    ON purchase_order_line_sources (consolidation_id) WHERE consolidation_id IS NOT NULL;

COMMENT ON TABLE purchase_order_line_sources IS
    'Which requisition lines an order line was raised for, and how much of each. What lets the cost of a receipt be split back across the cost centres that asked for it.';

-- ---------------------------------------------------------------------------
-- How much of an order line nobody asked for
-- ---------------------------------------------------------------------------
--
-- The buyer's own decision, made visible. A line ordered for 50 against 47 of
-- demand is three units of stock, charged to no department - and the three are
-- worth seeing, because a system that hides them is one where a rounding habit
-- becomes a stock problem nobody attributed.
--
-- A view rather than a column, for the reason `stock_quants_reconcile` is one:
-- the sources are the truth, and a stored total is a second thing to keep right.

CREATE VIEW purchase_order_line_allocation AS
SELECT l.id                                        AS order_line_id,
       l.order_id,
       l.variant_id,
       l.quantity_stock,
       COALESCE(sum(s.quantity), 0)                AS allocated,
       l.quantity_stock - COALESCE(sum(s.quantity), 0) AS unallocated,
       count(s.id)                                 AS requisition_lines
  FROM purchase_order_lines l
  LEFT JOIN purchase_order_line_sources s ON s.order_line_id = l.id
 GROUP BY l.id, l.order_id, l.variant_id, l.quantity_stock;

COMMENT ON VIEW purchase_order_line_allocation IS
    'How much of each order line was raised against a requisition and how much was the buyer buying for stock. Unallocated is not an error.';
