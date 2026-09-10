-- inventory 0011: what kind of discrepancy this was.
--
-- WHAT THIS IS
--
-- ADR 0006 section 7, and the last thing on its list. Every change to stock
-- that is not a purchase, a sale or a transfer goes through one movement into
-- or out of the inventory-loss location: a count difference, breakage, expiry,
-- a sample handed to a rep, a write-off. Until now all five were the same
-- movement and landed in the same account.
--
-- WHY THE TYPE IS THE POINT
--
-- A workspace that posts every discrepancy to one "inventory adjustment"
-- account ends the year with a number that grows and tells nobody anything.
-- Nobody can say whether it is theft, a warehouse that cannot count, or a
-- perishable range with the wrong shelf life - and those three have three
-- different answers, none of which is available from one total.
--
-- So the TYPE decides two things:
--
--   * which account the other side of the journal goes to, in place of the
--     workspace-wide inventory-adjustment default; and
--   * whether writing one needs somebody who may approve it.
--
-- The account is denormalised the way `account_mappings` is - id, number and
-- name together, no foreign key - because the chart of accounts belongs to
-- Books and an app holding a key into another app's schema is an app that can
-- never be uninstalled. See 0001 for the whole argument.
--
-- WHY `direction`
--
-- Most of these only go one way. Damage, expiry and theft take stock off the
-- shelf and can never put it back; found stock only ever puts it on. A count
-- difference is the one that genuinely goes both ways, and letting a person
-- book stock IN as "damage" is how a stock account comes to hold a credit
-- nobody can explain. The seed marks each type with the direction it means.
--
-- NO TRIGGERS, per ADR 0006. Every CHECK here is row-local.

CREATE TABLE adjustment_types (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Upper case and unique that way, as units and warehouses are. `DAMAGE`
    -- and `damage` are one type.
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,

    -- in | out | both. Which way stock may move under this reason.
    direction   TEXT NOT NULL DEFAULT 'both',

    -- Where the non-stock half of the journal lands. NULL falls back to
    -- whatever the workspace has mapped `InventoryAdjustment` to, which is what
    -- every adjustment did before this table existed.
    account_id     UUID,
    account_number TEXT,
    account_name   TEXT,

    -- Whether posting one needs `Inventory.Stock.Adjust.Approve` as well as
    -- `Inventory.Stock.Adjust`. Not a queue and not a second state: the
    -- adjustment is one movement, so "needs approval" is a question asked of
    -- the person pressing the button, at the moment they press it.
    needs_approval BOOLEAN NOT NULL DEFAULT FALSE,

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    -- Seeded by `config/defaults/inventory.toml`. A seeded type may be renamed,
    -- given an account, deactivated - everything but deleted, because moves
    -- already point at it and a workspace that removed "Count difference" would
    -- have nowhere to put the next count difference.
    is_system   BOOLEAN NOT NULL DEFAULT FALSE,

    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT adjustment_types_code_unique UNIQUE (code),
    CONSTRAINT adjustment_types_code_shape CHECK (
        code = upper(btrim(code)) AND char_length(code) BETWEEN 1 AND 24
    ),
    CONSTRAINT adjustment_types_named CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    ),
    CONSTRAINT adjustment_types_direction_known CHECK (
        direction IN ('in', 'out', 'both')
    ),
    -- The three account columns are one fact. Half an account - an id with no
    -- number to show beside it - is a picker that renders a blank row.
    CONSTRAINT adjustment_types_account_is_whole CHECK (
        (account_id IS NULL) = (account_number IS NULL)
        AND (account_id IS NULL) = (account_name IS NULL)
    ),
    CONSTRAINT adjustment_types_note_length CHECK (
        note IS NULL OR char_length(note) BETWEEN 1 AND 2000
    )
);

CREATE INDEX adjustment_types_active_idx
    ON adjustment_types (is_active, code);

-- ---------------------------------------------------------------------------
-- What a movement was for
-- ---------------------------------------------------------------------------
--
-- On the move rather than on a document of its own, because an adjustment IS a
-- movement - one line, one shelf, one reason - and inventing a header over it
-- would be a document whose only field is the one below.
--
-- Nullable, and NULL for every other kind of movement: a receipt is not an
-- adjustment and has no type. RESTRICT rather than SET NULL, because the whole
-- purpose of the column is to still be answerable in March.

ALTER TABLE stock_moves
    ADD COLUMN adjustment_type_id UUID REFERENCES adjustment_types (id) ON DELETE RESTRICT;

CREATE INDEX stock_moves_adjustment_type_idx
    ON stock_moves (adjustment_type_id, moved_on)
    WHERE adjustment_type_id IS NOT NULL;

-- What each reason has cost, which is the question the single account could
-- not answer. Value is signed: what left the shelf counts positive and what
-- came back counts negative, so a warehouse that loses forty and finds two
-- shows thirty-eight rather than forty-two.
CREATE VIEW adjustment_totals AS
SELECT t.id                 AS adjustment_type_id,
       t.code,
       t.name,
       m.moved_on,
       count(*)             AS move_count,
       sum(m.quantity)      AS quantity,
       sum(CASE WHEN fl.kind = 'inventory_loss' THEN -m.value ELSE m.value END) AS value
  FROM stock_moves m
  JOIN adjustment_types t ON t.id = m.adjustment_type_id
  JOIN locations fl ON fl.id = m.from_location_id
 WHERE m.state = 'done'
 GROUP BY t.id, t.code, t.name, m.moved_on;
