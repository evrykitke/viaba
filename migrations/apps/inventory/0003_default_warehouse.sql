-- inventory 0003: the warehouse a workspace always has.
--
-- WHY ONE IS MARKED
--
-- Every movement has two ends and one of them is almost always "the
-- warehouse". A receipt with no building to receive into is not an awkward
-- screen, it is a document that cannot exist, so the app seeds one - see
-- `config/defaults/inventory.toml`. What was missing is any way to say WHICH
-- one that is, and so any way for a receipt, a reordering rule or a till to
-- have a sensible answer without asking.
--
-- WHAT THE FLAG ACTUALLY BUYS
--
-- Two things, and they are both about what somebody may do to it.
--
--   * It cannot be switched off. A workspace with every warehouse retired has
--     nowhere for stock to be, and the failure surfaces on the first receipt
--     rather than on the screen that caused it.
--   * Its code and its step counts are fixed. The code is the first segment of
--     every location path in the building, and the step counts decide which
--     locations exist; both were chosen by the seed and both are load-bearing
--     for rows the workspace did not create. The NAME is not - "Main
--     warehouse" is ours and a workspace should call its building whatever it
--     calls it.
--
-- A workspace that outgrows the default adds a second warehouse, which is
-- editable in full. This is the floor, not a ceiling.
--
-- WHY A PARTIAL UNIQUE INDEX RATHER THAN A CHECK
--
-- "Exactly one row is true" is not a row-level constraint, and a trigger that
-- enforced it would fire on every write to say something about a different
-- row. `UNIQUE (is_default) WHERE is_default` is the one shape Postgres
-- answers directly: any number of false, at most one true.

ALTER TABLE warehouses
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN warehouses.is_default IS
    'The warehouse a document assumes. At most one; its code and step counts are fixed and it cannot be retired.';

CREATE UNIQUE INDEX IF NOT EXISTS warehouses_one_default
    ON warehouses (is_default)
 WHERE is_default;

-- Adopt whatever is already there.
--
-- A workspace provisioned before this migration has the seeded `WH` and no
-- flag. Marking it is the whole point of the backfill: the alternative is a
-- database where nothing is the default and every screen has to cope with
-- that. The lowest code where `WH` is gone, because a workspace that renamed
-- or replaced it still has exactly one building it means.
UPDATE warehouses
   SET is_default = TRUE
 WHERE id = (
        SELECT id
          FROM warehouses
         WHERE is_active
         ORDER BY (code <> 'WH'), code
         LIMIT 1
       )
   AND NOT EXISTS (SELECT 1 FROM warehouses WHERE is_default);
