-- hr 0001: departments, and the ones that are cost centres.
--
-- This app comes before the ledger that consumes it because a cost centre is a
-- dimension on a journal line, and a dimension has to be in the ledger's shape
-- from its first migration. It lives in a separate app reached through a port
-- because that is the hard case; building it inside Books would prove nothing.
-- See docs/adr/0006-apps-ports-and-defaults.md sections 6.4 and 9.
--
-- Nothing outside `hr` will hold a foreign key into it. Books stores a cost
-- centre's id, code and name on a journal line as a snapshot, so a rename does
-- not rewrite last year's report and `DROP SCHEMA hr CASCADE` stays safe.

CREATE TABLE departments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Generated (`DEPT-###`), unlike `master.parties.code` which is typed.
    -- Allocated from core.number_sequences inside the insert's transaction, so
    -- a rolled-back save returns the number. A workspace migrating from another
    -- system may type one instead.
    code            TEXT NOT NULL,

    name            TEXT NOT NULL,

    -- RESTRICT, not CASCADE: deleting a division must neither silently take the
    -- cost centres under it nor orphan them.
    parent_id       UUID REFERENCES departments (id) ON DELETE RESTRICT,

    -- Whether anything may be charged here. A flag rather than a second table:
    -- a department and a cost centre are the same thing seen twice.
    --
    -- FALSE by default, which is the interesting half — parent nodes usually
    -- are not cost centres, because posting to a parent and its children is how
    -- a report double-counts.
    is_cost_centre  BOOLEAN NOT NULL DEFAULT FALSE,

    -- SET NULL: deleting a user must not be blocked by a department they
    -- happened to manage.
    manager_user_id UUID REFERENCES core.users (id) ON DELETE SET NULL,

    is_active       BOOLEAN NOT NULL DEFAULT TRUE,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT departments_code_format CHECK (
        code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$' AND char_length(code) <= 40
    ),
    CONSTRAINT departments_name_present CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    ),

    -- Deeper cycles are the service's problem; a CHECK cannot walk a tree.
    CONSTRAINT departments_not_own_parent CHECK (parent_id IS DISTINCT FROM id)
);

-- Case-insensitive, so FIN and fin cannot be two departments that are one. Also
-- the belt to core.number_sequences' braces (ADR 0001 section 5 rule 4): it is
-- what stops an edited `start_at` putting a duplicate code in the ledger.
CREATE UNIQUE INDEX departments_code_key ON departments (lower(code));

-- For the delete path — "does anything hang off this row" — rather than the
-- list, which reads the whole table.
CREATE INDEX departments_parent ON departments (parent_id);

-- The picker's query, and the port's `list`.
CREATE INDEX departments_cost_centres
    ON departments (name)
    WHERE is_cost_centre AND is_active;
