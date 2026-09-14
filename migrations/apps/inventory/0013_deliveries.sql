-- inventory 0013: the goods going out.
--
-- 0012 added what was agreed. This is the event with the consequence: stock
-- leaves, the balance sheet loses it, and cost of sales gains it.
--
--   deliveries        goods leaving. The mirror of `receipts`
--   delivery_lines    what went on the pallet, and which move it became
--
-- WHERE THE GOODS LEAVE FROM
--
-- `from_location_id` is the warehouse's Output location for a two- or
-- three-step warehouse and its Stock location for a one-step one - the mirror
-- of a receipt landing at Input or Stock. The pick that puts goods into Output
-- is an internal transfer, which is a separate document for the reason Odoo
-- makes it one: what has been picked and what has been despatched are
-- different questions.
--
-- WHY A DELIVERY AND NOT JUST A MOVE
--
-- A stock move is one item leaving one place. A delivery is a van: eight items,
-- one despatch note, one customer, one date. Without the document there is no
-- row to hang the carrier's reference on, nothing to invoice against, and
-- nothing to print. The moves are still where the truth lives, and
-- `delivery_lines.move_id` is the thread from the paperwork to the ledger.
--
-- A SHORT SHIPMENT LEAVES THE REST ON THE ORDER
--
-- Agreed forty, shipped thirty: the delivery is for thirty, and the ten stay
-- outstanding on the order line. Nothing is stored about the remainder - it is
-- `quantity_stock - delivered`, worked out when it is asked, so a cancelled
-- delivery cannot leave a shortfall pointing at nothing.
--
-- WHAT IS DELIBERATELY ABSENT
--
-- No price and no revenue. A delivery moves goods and posts what those goods
-- COST; what the customer is charged is the invoice's business, against the tax
-- group in force on the invoice's own date. `value` below is cost, in the
-- workspace's own currency, and is the figure the journal posted.
--
-- No foreign key to `master.parties`, and none to `books.invoices`.

CREATE TABLE deliveries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `OUT-2026-00042`. Empty until posted, on the same terms as a receipt's:
    -- a draft somebody abandoned must not leave a hole in a series a customer
    -- and an auditor both read.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | done | cancelled
    state       TEXT NOT NULL DEFAULT 'draft',

    -- The order this is against. NULL is ordinary: a sample, a replacement, a
    -- counter sale that was never quoted.
    order_id    UUID REFERENCES sales_orders (id) ON DELETE RESTRICT,

    customer_id   UUID NOT NULL,
    customer_code TEXT NOT NULL,
    customer_name TEXT NOT NULL,

    warehouse_id UUID NOT NULL REFERENCES warehouses (id) ON DELETE RESTRICT,
    from_location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,

    despatched_on DATE NOT NULL,

    -- The carrier's consignment number, typed off their paperwork. What
    -- somebody searches for when the customer rings about a parcel.
    carrier_reference TEXT,
    note        TEXT,

    -- What the goods COST, in the workspace's own currency: the figure the
    -- journal posted. Not what they sold for. Stored so a list can total
    -- without reading lines.
    value       NUMERIC(19, 4) NOT NULL DEFAULT 0,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    posted_at   TIMESTAMPTZ,
    posted_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT deliveries_state_known CHECK (state IN ('draft', 'done', 'cancelled')),
    CONSTRAINT deliveries_customer_named CHECK (
        char_length(customer_code) BETWEEN 1 AND 40
        AND char_length(customer_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT deliveries_numbered_when_posted CHECK ((state = 'done') = (number <> '')),
    CONSTRAINT deliveries_note_length CHECK (note IS NULL OR char_length(note) <= 2000),
    CONSTRAINT deliveries_carrier_length CHECK (
        carrier_reference IS NULL OR char_length(carrier_reference) BETWEEN 1 AND 120
    )
);

CREATE UNIQUE INDEX deliveries_number ON deliveries (number) WHERE number <> '';
CREATE INDEX deliveries_order ON deliveries (order_id) WHERE order_id IS NOT NULL;
CREATE INDEX deliveries_customer ON deliveries (customer_id, despatched_on DESC);
CREATE INDEX deliveries_date ON deliveries (despatched_on DESC);
-- The customer rings and quotes the carrier's number.
CREATE INDEX deliveries_carrier ON deliveries (lower(carrier_reference))
    WHERE carrier_reference IS NOT NULL;

CREATE TABLE delivery_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    delivery_id UUID NOT NULL REFERENCES deliveries (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL,

    -- Which order line this satisfies. NULL for a delivery with no order, and
    -- for something that went out which nobody ordered.
    order_line_id UUID REFERENCES sales_order_lines (id) ON DELETE RESTRICT,

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    description TEXT NOT NULL,

    -- In the item's STOCK unit. Converted before it reaches this table, so
    -- nothing downstream of a delivery has to ask what unit a row is in.
    quantity    NUMERIC(19, 6) NOT NULL,

    -- Which batch left, where the item is tracked. Chosen rather than typed -
    -- unlike a receipt, where the number is the supplier's and new to us, a
    -- despatch picks from lots this workspace already holds.
    lot_id      UUID REFERENCES lots (id) ON DELETE RESTRICT,

    -- What one stock unit COST, in the workspace's own currency, worked out by
    -- the costing method when the move was applied. Written back after the
    -- move, because average and FIFO only know it then.
    unit_cost   NUMERIC(19, 4) NOT NULL DEFAULT 0,
    value       NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- The movement this line became, set at post. The thread from the
    -- paperwork to the stock ledger.
    move_id     UUID REFERENCES stock_moves (id) ON DELETE RESTRICT,

    CONSTRAINT delivery_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT delivery_lines_cost_not_negative CHECK (unit_cost >= 0),
    CONSTRAINT delivery_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    )
);

CREATE UNIQUE INDEX delivery_lines_position ON delivery_lines (delivery_id, line_no);
CREATE INDEX delivery_lines_variant ON delivery_lines (variant_id);
CREATE INDEX delivery_lines_order_line ON delivery_lines (order_line_id)
    WHERE order_line_id IS NOT NULL;
-- One move belongs to one delivery line. A second row would be one despatch
-- counted twice.
CREATE UNIQUE INDEX delivery_lines_move ON delivery_lines (move_id) WHERE move_id IS NOT NULL;
