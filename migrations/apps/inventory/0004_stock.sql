-- inventory 0004: the stock ledger, and the three tables it proves.
--
-- WHAT THIS FILE FINALLY BUILDS
--
-- 0001 said it in its own header: "This migration builds the vocabulary; the
-- moves themselves come next." This is next. Stock is never created or
-- destroyed - it moves between two locations, and the locations include the
-- ones that are not places. Everything below follows from that one sentence.
--
--   lots                which particular units these are, and when they expire
--   stock_moves         the ledger. One row per movement, append-only
--   stock_quants        how much is in one place. A CACHE the moves can prove
--   valuation_layers    what a receipt cost, and how much of it is left
--   layer_consumptions  which layer paid for which issue
--
-- WHY A QUANT EXISTS AT ALL
--
-- `stock_quants` is derivable: it is `SUM(quantity)` over `stock_moves` grouped
-- by variant, location and lot. It is stored because that sum is asked on every
-- screen and computing it per row is a scan per row. It is NOT the truth. The
-- moves are, and a quant that disagrees with them is a bug this schema can
-- DETECT - which is the entire practical payoff of modelling stock as double
-- entry, and is what `stock_quants_reconcile` at the foot of this file is for.
--
-- WHY NOTHING HERE IS EVER UPDATED
--
-- A move in state `done` is never updated and never deleted; a mistake is
-- corrected by a move the other way. Same rule as a posted journal, same
-- reason: a record that can be edited after the fact is not evidence of
-- anything.
--
-- It is enforced in `phonix_db::inventory::movement`, not by a trigger. There
-- is no statement in this codebase that updates a finished move except the one
-- that writes back where its journal landed, and that runs in the same
-- transaction as the move itself; every other write carries `WHERE state =
-- 'draft'` in its own text, where a reader can see it. A trigger would put the
-- rule in a place nobody reads while writing the query it governs.
--
-- WHY THE QUANTITY CARRIES NO SIGN
--
-- Direction is the two ends. A negative quantity would make every report ask
-- whether it is looking at a receipt of minus three or a return of three, and
-- those are different facts about a supplier.
--
-- NO FOREIGN KEY LEAVES THIS SCHEMA except into `core`. `journal_id` is a bare
-- `books.journals` id with nothing behind it, exactly as `account_mappings`
-- carries an account id - ADR 0001, and the reason an app can be uninstalled.

-- ---------------------------------------------------------------------------
-- Lots and serial numbers
-- ---------------------------------------------------------------------------
--
-- One table for both. A lot covers many units and a serial covers exactly one;
-- everything else about them is identical, and two tables would mean every
-- quant, every move and every recall query written twice.
--
-- The NUMBER is typed, never generated - it is the supplier's, printed on the
-- carton, and a number this system invented would match nothing on the box.
-- That is the same exception `items.barcode` takes, for the same reason.

CREATE TABLE lots (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Stock hangs off a variant, so a lot does too.
    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,

    number      TEXT NOT NULL,

    -- What makes FEFO possible and a recall a query. NULL for anything without
    -- a shelf life.
    expires_on  DATE,

    -- lot | serial. Snapshotted from the item, so a movement can be checked
    -- without reading two more tables to find out what kind of number this is.
    tracking    TEXT NOT NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT lots_number_present CHECK (char_length(number) BETWEEN 1 AND 64),
    -- A number with a space in it is a number a scanner reads as two.
    CONSTRAINT lots_number_no_space CHECK (number !~ '\s'),
    CONSTRAINT lots_tracking_known CHECK (tracking IN ('lot', 'serial'))
);

-- One number per variant. Two rows called LOT-8841 are two batches nobody can
-- tell apart on a recall.
CREATE UNIQUE INDEX lots_number ON lots (variant_id, lower(number));
CREATE INDEX lots_variant ON lots (variant_id);
-- FEFO reads this: the nearest expiry first, across everything still dated.
CREATE INDEX lots_expiry ON lots (expires_on) WHERE expires_on IS NOT NULL;

-- ---------------------------------------------------------------------------
-- The stock ledger
-- ---------------------------------------------------------------------------

CREATE TABLE stock_moves (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,

    -- Both ends, always. RESTRICT because deleting a location out from under a
    -- move would orphan half an entry, and the moves are the audit trail.
    from_location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,
    to_location_id   UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,

    lot_id      UUID REFERENCES lots (id) ON DELETE RESTRICT,

    -- Positive, always. See the header.
    quantity    NUMERIC(19, 6) NOT NULL,

    -- The item's stock unit as it was when this happened. A snapshot: the unit
    -- is frozen once stock exists, and this row is what proves it was.
    unit_id     UUID NOT NULL REFERENCES units (id) ON DELETE RESTRICT,

    -- draft | done | cancelled
    state       TEXT NOT NULL DEFAULT 'draft',

    -- The date the journal takes, which is not always today: goods that
    -- arrived on Friday and were keyed on Monday belong to Friday.
    moved_on    DATE NOT NULL,

    -- In the workspace's base currency, at Money's four decimal places.
    unit_cost   NUMERIC(19, 4) NOT NULL DEFAULT 0,
    -- `quantity * unit_cost`, rounded ONCE when this row was written. Stored
    -- rather than derived so that a stock report and a stock account cannot
    -- come to differ by a rounding nobody can account for.
    value       NUMERIC(19, 4) NOT NULL DEFAULT 0,

    reference   TEXT,

    -- The document behind it, on the same three-field terms a journal's source
    -- takes: "which receipt is this pallet" has to have an answer that does not
    -- involve a human. No foreign key - the documents are in this schema, but
    -- `source_doc_type` names which table and a key cannot be polymorphic.
    source_doc_type TEXT,
    source_doc_id   UUID,

    -- not_required | no_ledger | posted
    --
    -- Three states and none of them is a failure. A move whose kind changes
    -- what the workspace holds asks the `Ledger` port to post; where nobody
    -- implements it the move STILL HAPPENS and says so, because receiving goods
    -- is a warehouse fact and not an accounting one. Any other answer from the
    -- ledger - a closed period, an unmapped role - rolls the movement back, so
    -- a workspace that HAS a ledger never holds a stock figure its own stock
    -- account disagrees with.
    journal_state  TEXT NOT NULL DEFAULT 'not_required',

    -- A bare `books.journals` id. No foreign key: see the file header.
    journal_id     UUID,
    journal_number TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT stock_moves_quantity_positive CHECK (quantity > 0),
    CONSTRAINT stock_moves_two_ends CHECK (from_location_id <> to_location_id),
    CONSTRAINT stock_moves_state_known CHECK (state IN ('draft', 'done', 'cancelled')),
    CONSTRAINT stock_moves_cost_not_negative CHECK (unit_cost >= 0),
    CONSTRAINT stock_moves_reference_length CHECK (
        reference IS NULL OR char_length(reference) BETWEEN 1 AND 120
    ),
    CONSTRAINT stock_moves_source_is_whole CHECK (
        (source_doc_type IS NULL) = (source_doc_id IS NULL)
    ),
    CONSTRAINT stock_moves_journal_state_known CHECK (
        journal_state IN ('not_required', 'no_ledger', 'posted')
    ),
    -- A posted move names its journal, and an unposted one names none. The
    -- three columns are one fact and this keeps them from drifting apart.
    CONSTRAINT stock_moves_journal_is_whole CHECK (
        (journal_state = 'posted') = (journal_id IS NOT NULL)
        AND (journal_id IS NULL) = (journal_number IS NULL)
    )
);

-- What a quant is rebuilt from, and what a stock card is read from.
CREATE INDEX stock_moves_variant ON stock_moves (variant_id, moved_on DESC);
CREATE INDEX stock_moves_from ON stock_moves (from_location_id, variant_id) WHERE state = 'done';
CREATE INDEX stock_moves_to ON stock_moves (to_location_id, variant_id) WHERE state = 'done';
CREATE INDEX stock_moves_lot ON stock_moves (lot_id) WHERE lot_id IS NOT NULL;
CREATE INDEX stock_moves_source ON stock_moves (source_doc_type, source_doc_id)
    WHERE source_doc_id IS NOT NULL;
-- Reconciling the sub-ledger to the general ledger: a GROUP BY, not an
-- investigation. ADR 0006 section 6.7.
CREATE INDEX stock_moves_journal ON stock_moves (journal_id) WHERE journal_id IS NOT NULL;
CREATE INDEX stock_moves_date ON stock_moves (moved_on DESC);

-- ---------------------------------------------------------------------------
-- Quants
-- ---------------------------------------------------------------------------
--
-- RESERVED IS NOT GONE. A reservation holds stock for a picking that has not
-- happened; the units are still on the shelf and still on the balance sheet.
-- `quantity` is what a stock count is checked against and `quantity - reserved`
-- is what a sales line is checked against, and conflating the two is how a
-- warehouse is told it has nothing while a full pallet sits in the aisle.
--
-- NEGATIVE STOCK IS REFUSED for a location that is ours - ADR 0006 section 6.6.
-- A shelf below zero holds units that were never received, which have no cost,
-- which makes every valuation after that moment guesswork. The counterpart
-- locations have no such floor and go as negative as the business is old: a
-- vendor location at -4,000 is a statement that four thousand units have been
-- bought, and that is exactly what it should say.
--
-- The floor is enforced in `app_inventory::quant::take`, which every write goes
-- through, and it cannot be a CHECK here because whether a location is ours is
-- a fact about another table. It is not a trigger either: the rule needs to say
-- HOW MANY are short, and a constraint violation cannot. What refuses a short
-- issue is the same code that tells the person how many they are missing.

CREATE TABLE stock_quants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,
    lot_id      UUID REFERENCES lots (id) ON DELETE RESTRICT,

    quantity    NUMERIC(19, 6) NOT NULL DEFAULT 0,
    reserved    NUMERIC(19, 6) NOT NULL DEFAULT 0,

    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- The floor on `quantity` is code, not a constraint: see the note above.
    -- These two are row-local facts, which is exactly what a CHECK is for.
    CONSTRAINT stock_quants_reserved_not_negative CHECK (reserved >= 0),
    -- The same units may not be promised twice. Written as "nothing reserved,
    -- or no more than is there" rather than `reserved <= quantity`, because a
    -- counterpart location is *meant* to go negative: a vendor location at
    -- -4,000 reserves nothing and would fail the simpler form on every receipt.
    CONSTRAINT stock_quants_reserved_is_there CHECK (reserved = 0 OR reserved <= quantity)
);

-- One row per (variant, location, lot). Two partial indexes rather than one
-- constraint, because NULL is not equal to NULL and an untracked item's rows
-- would otherwise multiply quietly, one per movement.
CREATE UNIQUE INDEX stock_quants_lotted
    ON stock_quants (variant_id, location_id, lot_id) WHERE lot_id IS NOT NULL;
CREATE UNIQUE INDEX stock_quants_unlotted
    ON stock_quants (variant_id, location_id) WHERE lot_id IS NULL;

CREATE INDEX stock_quants_location ON stock_quants (location_id);
CREATE INDEX stock_quants_lot ON stock_quants (lot_id) WHERE lot_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Valuation layers
-- ---------------------------------------------------------------------------
--
-- A layer per receipt, whatever the costing method. Under FIFO the layers are
-- consumed oldest first and each carries its own cost; under average and
-- standard there is one cost for everything and the layers are STILL written,
-- because a layer is also the record of what a receipt cost - and a workspace
-- that changes costing method next year would otherwise have thrown that away.

CREATE TABLE valuation_layers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    move_id     UUID NOT NULL REFERENCES stock_moves (id) ON DELETE RESTRICT,
    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    lot_id      UUID REFERENCES lots (id) ON DELETE RESTRICT,

    -- What came in.
    quantity    NUMERIC(19, 6) NOT NULL,
    -- What has not been consumed yet. Zero for a spent layer.
    remaining   NUMERIC(19, 6) NOT NULL,

    unit_cost   NUMERIC(19, 4) NOT NULL,
    -- Rounded once, when this row was written.
    value       NUMERIC(19, 4) NOT NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT valuation_layers_quantity_positive CHECK (quantity > 0),
    CONSTRAINT valuation_layers_remaining_in_range CHECK (
        remaining >= 0 AND remaining <= quantity
    ),
    CONSTRAINT valuation_layers_cost_not_negative CHECK (unit_cost >= 0)
);

-- One move brings one item in once.
CREATE UNIQUE INDEX valuation_layers_move ON valuation_layers (move_id);
-- What FIFO reads: the oldest layer of this variant with anything left in it.
CREATE INDEX valuation_layers_open
    ON valuation_layers (variant_id, created_at) WHERE remaining > 0;
CREATE INDEX valuation_layers_lot ON valuation_layers (lot_id) WHERE lot_id IS NOT NULL;

-- Which layer paid for which issue.
--
-- The row an auditor asks for: "these forty units went out at 2.15 - show me
-- what they cost coming in". Without it a FIFO cost is a number the system
-- asserts, and with it the number is a join.
CREATE TABLE layer_consumptions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    layer_id    UUID NOT NULL REFERENCES valuation_layers (id) ON DELETE RESTRICT,
    move_id     UUID NOT NULL REFERENCES stock_moves (id) ON DELETE RESTRICT,

    quantity    NUMERIC(19, 6) NOT NULL,
    unit_cost   NUMERIC(19, 4) NOT NULL,
    value       NUMERIC(19, 4) NOT NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT layer_consumptions_quantity_positive CHECK (quantity > 0),

    -- One layer pays for one move once. A second row would be the same units
    -- consumed twice.
    UNIQUE (layer_id, move_id)
);

CREATE INDEX layer_consumptions_move ON layer_consumptions (move_id);

-- ---------------------------------------------------------------------------
-- The proof
-- ---------------------------------------------------------------------------
--
-- Every quant, beside what the moves say it should be. This is the payoff of
-- keeping stock as double entry, and it exists so that "is the cache right" is
-- a query anybody can run rather than a belief.
--
-- Empty is correct. A row here is a bug, and it names the variant, the location
-- and the size of the disagreement.

CREATE VIEW stock_quants_reconcile AS
WITH ledger AS (
    SELECT variant_id, location_id, lot_id, sum(quantity) AS quantity
      FROM (
            SELECT variant_id, to_location_id AS location_id, lot_id, quantity
              FROM stock_moves WHERE state = 'done'
             UNION ALL
            SELECT variant_id, from_location_id AS location_id, lot_id, -quantity
              FROM stock_moves WHERE state = 'done'
           ) entries
     GROUP BY variant_id, location_id, lot_id
)
SELECT COALESCE(q.variant_id, l.variant_id)   AS variant_id,
       COALESCE(q.location_id, l.location_id) AS location_id,
       COALESCE(q.lot_id, l.lot_id)           AS lot_id,
       COALESCE(q.quantity, 0)                AS quant_quantity,
       COALESCE(l.quantity, 0)                AS ledger_quantity,
       COALESCE(q.quantity, 0) - COALESCE(l.quantity, 0) AS difference
  FROM stock_quants q
  FULL OUTER JOIN ledger l
    ON  l.variant_id = q.variant_id
    AND l.location_id = q.location_id
    AND l.lot_id IS NOT DISTINCT FROM q.lot_id
 WHERE COALESCE(q.quantity, 0) <> COALESCE(l.quantity, 0);

COMMENT ON VIEW stock_quants_reconcile IS
    'Quants that disagree with the movement history. Empty is correct; a row is a bug.';
