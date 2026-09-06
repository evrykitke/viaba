-- books 0003: the general ledger.
--
-- Accounts (0002) are the vocabulary. This is what is written in it.
--
-- FIVE RULES, AND WHERE EACH ONE LIVES
--
--   1. Double entry            enforced in the type, not here - a JournalEntry
--                              cannot be constructed unbalanced, so no code
--                              path reaches this table with one. The CHECK
--                              below is the belt to that brace.
--   2. Append-only             no UPDATE and no DELETE on a posted journal. A
--                              mistake is corrected by a reversing journal that
--                              names the one it reverses.
--   3. Every journal names     source_app, source_doc_type, source_doc_id.
--      its source              Reconciling a sub-ledger to the general ledger
--                              is then a GROUP BY rather than an investigation.
--   4. Periods lock            a journal dated inside a closed period is
--                              refused. Not a warning.
--   5. Dimensions are          a line carries a SET of dimension values, in
--      orthogonal              their own table, not two named columns. See ADR
--                              0006 section 6.4 for why this is the thing the
--                              incumbents get wrong.
--
-- WHY A PERIOD IS A ROW AND NOT A DERIVED MONTH
--
-- Because closing is an act somebody performs, on a date, and "which periods
-- are shut" is the question a posting asks a hundred times a day. Deriving it
-- from a year-end setting means every posting recomputes a calendar, and there
-- is nowhere to record who closed January or when.

-- The accounting calendar. Periods do not overlap and are closed by hand.
CREATE TABLE periods (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `2026-03`. Generated when a year is opened, and shown wherever a journal
    -- names its period.
    label       TEXT NOT NULL,

    starts_on   DATE NOT NULL,
    ends_on     DATE NOT NULL,

    -- Open until somebody shuts it. Reopening is allowed and is audited: a
    -- period that could never be reopened would make a late correction
    -- impossible, and the correction would happen in a spreadsheet instead.
    is_closed   BOOLEAN NOT NULL DEFAULT FALSE,

    closed_at   TIMESTAMPTZ,
    closed_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT periods_dates_ordered CHECK (ends_on >= starts_on),
    CONSTRAINT periods_closed_whole CHECK (
        (NOT is_closed AND closed_at IS NULL)
        OR (is_closed AND closed_at IS NOT NULL)
    )
);

-- One period per day, enforced rather than assumed: two periods covering the
-- same date would make "is this date closed" a question with two answers.
CREATE EXTENSION IF NOT EXISTS btree_gist;
ALTER TABLE periods
    ADD CONSTRAINT periods_no_overlap
    EXCLUDE USING gist (daterange(starts_on, ends_on, '[]') WITH &&);

CREATE UNIQUE INDEX periods_label_key ON periods (lower(label));

-- The lookup every posting makes: which period covers this date.
CREATE INDEX periods_range ON periods (starts_on, ends_on);

-- A journal: a balanced set of lines, posted on a date, from a document.
CREATE TABLE journals (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Allocated at post from core.number_sequences, in the same transaction as
    -- the write, so a rolled-back post returns the number. There is no draft
    -- journal: a journal exists because it was posted.
    number         TEXT NOT NULL,

    -- The date the transaction belongs to, which is not the date it was
    -- entered. `posted_at` is the second one, and reports read this one.
    entry_date     DATE NOT NULL,

    -- Resolved at post and stored. Recomputing it later would move a journal
    -- into a different period if somebody ever redrew the calendar, and a
    -- report that changes after it was filed is the thing this ledger exists
    -- to prevent.
    period_id      UUID NOT NULL REFERENCES periods (id) ON DELETE RESTRICT,

    narration      TEXT NOT NULL,

    -- Rule 3. `source_app` is an app id, not a schema reference: no foreign key
    -- leaves this schema, so a sub-ledger in another app can name its own
    -- document without books knowing that app's tables exist.
    source_app     TEXT NOT NULL,
    source_doc_type TEXT NOT NULL,
    source_doc_id  UUID,

    -- Set on a journal that reverses another. The reversed journal is not
    -- marked: it is append-only, and a column somebody has to write on an old
    -- row is an update to a posted document.
    reverses_id    UUID REFERENCES journals (id) ON DELETE RESTRICT,

    posted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    posted_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT journals_narration_present CHECK (
        char_length(btrim(narration)) BETWEEN 1 AND 500
    ),
    CONSTRAINT journals_source_app_present CHECK (
        char_length(source_app) BETWEEN 1 AND 40
    ),
    CONSTRAINT journals_source_doc_type_present CHECK (
        char_length(source_doc_type) BETWEEN 1 AND 40
    ),
    -- A journal cannot reverse itself. Longer chains - a reversal of a reversal
    -- - are legitimate and are left alone.
    CONSTRAINT journals_not_own_reversal CHECK (reverses_id IS DISTINCT FROM id)
);

CREATE UNIQUE INDEX journals_number_key ON journals (lower(number));

-- "What is in March", and the period close's own check.
CREATE INDEX journals_period ON journals (period_id, entry_date);

-- Rule 3's payoff: reconciling a sub-ledger is this index.
CREATE INDEX journals_source ON journals (source_app, source_doc_type, source_doc_id);

-- At most one live reversal per journal. Reversing the same document twice is
-- a double correction, and it is always a mistake.
CREATE UNIQUE INDEX journals_one_reversal ON journals (reverses_id)
    WHERE reverses_id IS NOT NULL;

-- One side of one posting.
CREATE TABLE journal_lines (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    journal_id   UUID NOT NULL REFERENCES journals (id) ON DELETE CASCADE,

    -- Where the line sits on screen and in an export. Stored, because a journal
    -- read back in a different order is a journal somebody has to re-read.
    position     INTEGER NOT NULL,

    account_id   UUID NOT NULL REFERENCES accounts (id) ON DELETE RESTRICT,

    -- 'debit' or 'credit', with a non-negative amount. Not one signed column:
    -- an accountant reads a journal in two columns, and a signed amount makes
    -- "which side is this" a sum rather than a fact.
    side         TEXT NOT NULL,

    -- The six-column currency snapshot, per ADR 0001 section 3. `amount` is as
    -- transacted; the rest is what it was worth in the workspace's own currency
    -- on the day, recorded so nothing has to be recomputed from a later rate.
    currency_code      TEXT NOT NULL REFERENCES core.currencies (code),
    amount             NUMERIC(19, 4) NOT NULL,
    base_currency_code TEXT NOT NULL REFERENCES core.currencies (code),
    base_amount        NUMERIC(19, 4) NOT NULL,
    exchange_rate      NUMERIC(20, 10) NOT NULL,
    rate_date          DATE NOT NULL,

    -- What this line is for, where the narration on the journal is not enough.
    memo         TEXT,

    CONSTRAINT journal_lines_side_valid CHECK (side IN ('debit', 'credit')),

    -- Rule 1's belt. A zero line is refused too: it moves nothing and is
    -- always either a mistake or a placeholder somebody forgot to fill in.
    CONSTRAINT journal_lines_amount_positive CHECK (amount > 0),
    CONSTRAINT journal_lines_base_amount_positive CHECK (base_amount > 0),
    CONSTRAINT journal_lines_rate_positive CHECK (exchange_rate > 0),
    CONSTRAINT journal_lines_currency_format CHECK (currency_code ~ '^[A-Z]{3}$'),
    CONSTRAINT journal_lines_base_currency_format CHECK (base_currency_code ~ '^[A-Z]{3}$'),
    CONSTRAINT journal_lines_memo_length CHECK (memo IS NULL OR char_length(memo) <= 500),

    CONSTRAINT journal_lines_position_unique UNIQUE (journal_id, position)
);

-- Every report and every account enquiry.
CREATE INDEX journal_lines_account ON journal_lines (account_id);
CREATE INDEX journal_lines_journal ON journal_lines (journal_id, position);

-- Rule 5. A line carries a set of dimension values rather than two named
-- columns, which is the ceiling every mid-tier package hits in year two.
--
-- The value is a SNAPSHOT - id, code and name - resolved through the owning
-- app's port at the moment of posting. Books holds no foreign key into `hr`,
-- and a department renamed next year must not rewrite last year's report.
CREATE TABLE journal_line_dimensions (
    line_id     UUID NOT NULL REFERENCES journal_lines (id) ON DELETE CASCADE,

    -- 'cost_centre' today. A second kind - project, location, fund - is a new
    -- value here and a resolver above, not a migration.
    dimension   TEXT NOT NULL,

    value_id    UUID NOT NULL,
    value_code  TEXT NOT NULL,
    value_name  TEXT NOT NULL,

    PRIMARY KEY (line_id, dimension),

    CONSTRAINT journal_line_dimensions_kind_present CHECK (
        char_length(dimension) BETWEEN 1 AND 40
    ),
    CONSTRAINT journal_line_dimensions_code_present CHECK (
        char_length(btrim(value_code)) BETWEEN 1 AND 40
    ),
    CONSTRAINT journal_line_dimensions_name_present CHECK (
        char_length(btrim(value_name)) BETWEEN 1 AND 200
    )
);

-- "Everything charged to Finance", which is the report a cost centre exists
-- for and the reason the dimension is a row rather than a column.
CREATE INDEX journal_line_dimensions_value
    ON journal_line_dimensions (dimension, value_id);
