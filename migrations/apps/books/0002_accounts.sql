-- books 0002: the chart of accounts.
--
-- What a workspace may post to. The ledger itself - journals and their lines -
-- comes next; this is the vocabulary that ledger is written in, and it lands
-- first because a journal line references an account and nothing references a
-- journal line.
--
-- WHY THE TYPE AND NOT THE NUMBER
--
-- `account_type` is what the software reasons about: it decides the normal
-- balance, whether the balance closes into retained earnings at year end, and
-- whether a sub-ledger owns the account. `number` decides only the order things
-- appear in a report.
--
-- That split is deliberate and it is the one most small packages get wrong. An
-- accountant WILL renumber the chart - they have used 1200 for the bank since
-- 1994 and the new system is not going to change their minds - and when they
-- do, nothing may break. Inferring "4000-4999 is revenue" from the digits makes
-- a renumbering a silent restatement of the profit and loss account.
--
-- The ranges in ADR 0006 section 4 are therefore documentation for the default
-- chart in `config/defaults/books.toml`, not a rule enforced here. A workspace
-- numbering its revenue in the 7000s is unusual, not wrong.
--
-- NO FOREIGN KEY INTO ANOTHER APP
--
-- Same rule as 0001: `core` is the one schema this may point at. A cost centre
-- on a journal line will be a snapshot resolved through the `CostCentres` port,
-- never an FK into `hr`.

CREATE TABLE accounts (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Typed, never generated. Unlike a department code there is nothing
    -- sensible to allocate: the default chart supplies a number, and an
    -- accountant adding an account has one in mind before they have a name.
    number        TEXT NOT NULL,

    name          TEXT NOT NULL,

    -- Enumerated rather than free text, so a typo cannot invent a class of
    -- account that no report knows how to total. Adding a type is a migration,
    -- which is the right amount of friction for a decision this load-bearing.
    account_type  TEXT NOT NULL,

    description   TEXT,

    -- Retired rather than deleted, once anything has been posted here. An
    -- account with history is never removed - the history would stop naming
    -- anything - so this is how a chart is tidied.
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,

    -- Whether this row came from `config/defaults/books.toml`. Provenance only:
    -- a seeded account may be renamed, renumbered, retired or deleted like any
    -- other. A default is what a workspace starts with, not what it is held to.
    is_default    BOOLEAN NOT NULL DEFAULT FALSE,

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by    UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT accounts_number_format CHECK (
        number ~ '^[A-Za-z0-9][A-Za-z0-9_.-]*$' AND char_length(number) <= 20
    ),
    CONSTRAINT accounts_name_present CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    ),
    CONSTRAINT accounts_description_length CHECK (
        description IS NULL OR char_length(description) <= 500
    ),

    -- Every variant of `app_books::account::AccountType`. The four contra types
    -- are in here for the same reason they exist in Rust: accumulated
    -- depreciation is an asset that carries a credit balance, and a chart that
    -- cannot say so records depreciation backwards.
    CONSTRAINT accounts_type_valid CHECK (account_type IN (
        'cash',
        'bank',
        'accounts_receivable',
        'inventory',
        'prepaid_expense',
        'other_current_asset',
        'fixed_asset',
        'accumulated_depreciation',
        'contra_asset',
        'intangible_asset',
        'other_asset',
        'accounts_payable',
        'goods_received_not_invoiced',
        'tax_payable',
        'accrued_liability',
        'other_current_liability',
        'long_term_liability',
        'equity',
        'retained_earnings',
        'contra_equity',
        'revenue',
        'contra_revenue',
        'other_income',
        'cost_of_sales',
        'operating_expense',
        'depreciation',
        'other_expense',
        'income_tax'
    ))
);

-- Case-insensitive, so 1200a and 1200A cannot be two accounts that are one.
-- Also what makes seeding the default chart idempotent: the insert is
-- ON CONFLICT DO NOTHING against this index, so a redeploy never puts back an
-- account somebody deleted and never overwrites one they edited.
CREATE UNIQUE INDEX accounts_number_key ON accounts (lower(number));

-- Reports total by type, and the posting routines ask for "the receivables
-- one". Both are this index.
CREATE INDEX accounts_type ON accounts (account_type);

-- The account picker: active accounts, in the order an accountant expects to
-- read them.
CREATE INDEX accounts_active_number ON accounts (number) WHERE is_active;
