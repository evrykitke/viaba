-- inventory 0010: the transfer, and the stock that is in neither building.
--
-- WHAT THIS IS
--
-- ADR 0006 section 7. Stock moved between two of the workspace's own locations
-- - between warehouses, or between two rooms of one. The last document on
-- section 7's list that was still on paper.
--
-- WHY IT IS ONE DOCUMENT AND NOT TWO ADJUSTMENTS
--
-- Because of the middle. A lorry that left Bristol on Tuesday and arrives in
-- Leeds on Thursday is carrying stock that is at neither address on Wednesday,
-- and it is still ours and still on the balance sheet. "Subtract here, add
-- there" has nowhere to put Wednesday: either the stock is counted twice, or it
-- does not exist for two days, and both are wrong in a way that a stock count
-- discovers and a stock account cannot explain.
--
-- So a transfer is TWO movements against ONE document, and the place in between
-- is a real location of kind `transit` - rootless, in no warehouse, so no
-- warehouse total includes it, and carrying `AccountRole::InventoryInTransit`
-- so the balance sheet does. Both were declared in 0001 and until now nothing
-- posted to either.
--
--   despatch   origin  -> Transit    credit Inventory, debit InventoryInTransit
--   arrive     Transit -> destination   credit InventoryInTransit, debit Inventory
--
-- Neither movement is written here. Both are `stock::apply`, which already
-- values the move, spends or opens the layers, and files the journal - the
-- whole of what this document adds is the pairing, and the record that the
-- second half has not happened yet.
--
-- WHY A LINE CARRIES `despatched` AND `received` SEPARATELY
--
-- Because the difference is the point. `despatched - received` is what is on
-- the lorry, and a transfer where the two are equal is a transfer that has
-- arrived. Something that left and never turned up stays visible as a positive
-- difference rather than disappearing, and is written off from the Transit
-- location by an ordinary adjustment - which is the correct account for lost
-- goods and not the destination warehouse's shrinkage.
--
--   stock_transfers        one journey: two ends, two dates, one number
--   stock_transfer_lines   what is on it, and how far each line has got
--
-- NO TRIGGERS, per ADR 0006. Every CHECK here is row-local; that a line's
-- movements exist is enforced by the code that writes them.

-- ---------------------------------------------------------------------------
-- The journey
-- ---------------------------------------------------------------------------
--
-- Four states, and the third is only reachable through the second. A draft is
-- a plan - what a picking list holds before anybody loads anything. `Cancelled`
-- is reachable only from `draft`: once stock has left the shelf the document
-- cannot be un-made, and a load that turned back is received back into the
-- location it came from rather than cancelled.

CREATE TABLE stock_transfers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `INT-2026-00042`. Empty until despatched: allocated when the stock
    -- actually leaves, not when somebody opens the form, so an abandoned draft
    -- leaves no hole in a series.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | in_transit | done | cancelled
    state       TEXT NOT NULL DEFAULT 'draft',

    from_location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,
    to_location_id   UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,

    -- The middle. A column rather than a lookup at posting time, because the
    -- movement out and the movement back have to name the same place even if
    -- somebody adds a second transit location between Tuesday and Thursday.
    transit_location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,

    -- Snapshots, so a list draws without three joins to a recursive tree.
    from_path   TEXT NOT NULL,
    to_path     TEXT NOT NULL,

    -- The day it is planned for. `despatched_on` and `arrived_on` are what
    -- actually happened, and are the dates the two journals carry.
    planned_on    DATE NOT NULL,
    despatched_on DATE,
    arrived_on    DATE,

    -- A consignment note, a van registration, a courier's tracking number.
    -- Whatever somebody would search for when asked where the pallet is.
    reference   TEXT,
    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    despatched_by UUID REFERENCES core.users (id) ON DELETE SET NULL,
    arrived_by    UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT stock_transfers_state_known CHECK (
        state IN ('draft', 'in_transit', 'done', 'cancelled')
    ),
    -- A transfer to where it already is is not a transfer.
    CONSTRAINT stock_transfers_ends_differ CHECK (from_location_id <> to_location_id),
    CONSTRAINT stock_transfers_numbered_when_gone CHECK (
        (state IN ('in_transit', 'done')) = (number <> '')
    ),
    CONSTRAINT stock_transfers_dated_when_gone CHECK (
        (state IN ('in_transit', 'done')) = (despatched_on IS NOT NULL)
    ),
    -- Arriving before leaving is a keying error, and it would file two
    -- journals in the wrong order.
    CONSTRAINT stock_transfers_arrives_after_despatch CHECK (
        arrived_on IS NULL OR despatched_on IS NULL OR arrived_on >= despatched_on
    ),
    CONSTRAINT stock_transfers_reference_length CHECK (
        reference IS NULL OR char_length(reference) BETWEEN 1 AND 120
    ),
    CONSTRAINT stock_transfers_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    )
);

CREATE UNIQUE INDEX stock_transfers_number ON stock_transfers (number) WHERE number <> '';
CREATE INDEX stock_transfers_date ON stock_transfers (planned_on DESC);
CREATE INDEX stock_transfers_from ON stock_transfers (from_location_id);
CREATE INDEX stock_transfers_to ON stock_transfers (to_location_id);
-- The screen somebody opens to ask what is on the road.
CREATE INDEX stock_transfers_open ON stock_transfers (planned_on DESC)
    WHERE state IN ('draft', 'in_transit');

-- ---------------------------------------------------------------------------
-- What is on it
-- ---------------------------------------------------------------------------
--
-- `despatched` and `received` are advanced by deltas, never set to totals, for
-- the reason an order's `received` is: two people receiving two pallets of the
-- same line in the same minute must both count.
--
-- The two `move_id` columns are what make despatching or receiving twice
-- harmless. A line that already carries the movement for the act being retried
-- is skipped, which is the same guard the receipt uses and for the same reason:
-- each line is its own `stock::apply` and its own transaction, so a failure
-- halfway leaves the lines before it moved.

CREATE TABLE stock_transfer_lines (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transfer_id UUID NOT NULL REFERENCES stock_transfers (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL,

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    lot_id      UUID REFERENCES lots (id) ON DELETE RESTRICT,

    -- Snapshot, so a document reads what it said at the time.
    description TEXT NOT NULL,

    quantity    NUMERIC(19, 6) NOT NULL,
    despatched  NUMERIC(19, 6) NOT NULL DEFAULT 0,
    received    NUMERIC(19, 6) NOT NULL DEFAULT 0,

    despatch_move_id UUID REFERENCES stock_moves (id) ON DELETE SET NULL,
    arrival_move_id  UUID REFERENCES stock_moves (id) ON DELETE SET NULL,

    CONSTRAINT stock_transfer_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT stock_transfer_lines_despatched_within CHECK (
        despatched >= 0 AND despatched <= quantity
    ),
    -- What arrived cannot exceed what left. More turning up than was sent is a
    -- count error at one end or the other, and it is found by counting rather
    -- than absorbed by this document.
    CONSTRAINT stock_transfer_lines_received_within CHECK (
        received >= 0 AND received <= despatched
    ),
    CONSTRAINT stock_transfer_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 200
    ),
    CONSTRAINT stock_transfer_lines_numbered CHECK (line_no >= 1),
    CONSTRAINT stock_transfer_lines_unique_position UNIQUE (transfer_id, line_no)
);

CREATE INDEX stock_transfer_lines_transfer ON stock_transfer_lines (transfer_id);
CREATE INDEX stock_transfer_lines_variant ON stock_transfer_lines (variant_id);
CREATE INDEX stock_transfer_lines_lot ON stock_transfer_lines (lot_id) WHERE lot_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- What is on the road
-- ---------------------------------------------------------------------------
--
-- The question the transit account asks, answered from the documents rather
-- than from the Transit location's quants: a quant says how much is in transit,
-- and this says which journeys it belongs to and how long each has been going.
-- A stock account reconciles against the first; a person chasing a pallet needs
-- the second.

CREATE VIEW stock_in_transit AS
SELECT t.id AS transfer_id,
       t.number,
       t.from_path,
       t.to_path,
       t.despatched_on,
       sum(l.despatched - l.received) AS quantity,
       count(*) FILTER (WHERE l.despatched > l.received) AS open_lines
  FROM stock_transfers t
  JOIN stock_transfer_lines l ON l.transfer_id = t.id
 WHERE t.state = 'in_transit'
 GROUP BY t.id, t.number, t.from_path, t.to_path, t.despatched_on
HAVING sum(l.despatched - l.received) > 0;

COMMENT ON VIEW stock_in_transit IS
    'Journeys with stock still on them: what left, has not arrived, and is on the balance sheet in neither building.';
