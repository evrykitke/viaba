-- books 0004: which account a sub-ledger's posting lands on.
--
-- ACCOUNT DETERMINATION, WHICH IS THE NAME FOR THIS
--
-- A goods receipt knows it is increasing stock and increasing goods-received-
-- not-invoiced. It does not know, and must not know, that those are 1200 and
-- 2010 in this workspace - the chart belongs to Books and an accountant will
-- renumber it. So a sub-ledger names a ROLE and this table says which account
-- that role means here.
--
-- Getting this backwards - letting each sub-ledger store account ids - is how a
-- chart becomes unrenumberable, because every module would be holding a copy of
-- a decision that is supposed to live in one place.
--
-- The roles themselves are `phonix_ports::AccountRole`, which is a closed set
-- on purpose: a sub-ledger that could ask for an arbitrary account would be
-- reaching into the chart rather than through the port.
--
-- WHY THE ROW IS THE MAPPING AND NOT THE ACCOUNT'S OWN COLUMN
--
-- Because a role is not a property of an account. "The inventory control
-- account" is a decision about which of possibly several inventory accounts the
-- stock ledger reconciles to, and a workspace with three of them has to be able
-- to say which. A flag on the account could not express that, and two accounts
-- flagged would be a question with two answers.

CREATE TABLE account_roles (
    -- One row per role. The primary key is the role, which is what makes "two
    -- accounts for the same purpose" unrepresentable rather than merely wrong.
    role        TEXT PRIMARY KEY,

    -- RESTRICT: an account something posts to may not be deleted out from
    -- under it. Books' own service already refuses to delete any account, and
    -- this is the database saying the same thing.
    account_id  UUID NOT NULL REFERENCES accounts (id) ON DELETE RESTRICT,

    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT account_roles_role_present CHECK (
        char_length(role) BETWEEN 1 AND 40
    )
);

-- "What else posts here", which is the question asked before retiring an
-- account and the one a mapping screen answers per row.
CREATE INDEX account_roles_account ON account_roles (account_id);

-- The default mapping, derived from the chart this workspace was seeded with.
--
-- Each role takes the lowest-numbered active account of the type that means it.
-- Lowest rather than any, so a chart with 1200 Inventory and 1210 Inventory in
-- transit maps the control account to 1200 and not to whichever row came back
-- first. ON CONFLICT DO NOTHING, so a workspace that has already chosen keeps
-- its choice through every later deploy.
--
-- Roles with no matching account are simply absent, and the port answers
-- `UnmappedRole` for them - which is a sentence a screen can show rather than a
-- foreign-key violation from four layers down.
INSERT INTO account_roles (role, account_id)
SELECT mapping.role, chosen.id
  FROM (VALUES
        ('inventory',                   'inventory'),
        ('goods_received_not_invoiced', 'goods_received_not_invoiced'),
        ('accounts_payable',            'accounts_payable'),
        ('cost_of_sales',               'cost_of_sales'),
        -- Purchase price variance, landed cost and inventory adjustment are all
        -- cost-of-sales accounts in the default chart; the numbers below pick
        -- the specific ones it seeds rather than the first of the class.
        ('purchase_price_variance',     NULL),
        ('landed_cost',                 NULL),
        ('inventory_adjustment',        NULL),
        ('inventory_in_transit',        NULL)
       ) AS mapping (role, account_type)
  JOIN LATERAL (
        SELECT a.id
          FROM accounts a
         WHERE a.is_active
           AND a.account_type = mapping.account_type
         ORDER BY a.number
         LIMIT 1
       ) AS chosen ON TRUE
 WHERE mapping.account_type IS NOT NULL
    ON CONFLICT DO NOTHING;

-- The four the type alone cannot pick out, by the number the default chart
-- gives them. A workspace that renumbered before this migration ran simply gets
-- no mapping for these, which is the honest outcome: better an unmapped role
-- with a message than a guess that puts freight in the wrong account.
INSERT INTO account_roles (role, account_id)
SELECT mapping.role, a.id
  FROM (VALUES
        ('purchase_price_variance', '5230'),
        ('landed_cost',             '5090'),
        ('inventory_adjustment',    '5200'),
        ('inventory_in_transit',    '1240')
       ) AS mapping (role, number)
  JOIN accounts a ON a.number = mapping.number AND a.is_active
    ON CONFLICT DO NOTHING;
