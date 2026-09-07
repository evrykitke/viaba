-- inventory 0001: what the workspace stocks, and where it can be.
--
-- THE ONE IDEA THIS SCHEMA IS BUILT ON
--
-- Stock is never created or destroyed; it only moves between locations, and
-- the locations include the ones that are not places - the supplier, the
-- customer, inventory loss, production. A receipt is a move FROM a vendor
-- location. A count difference is a move TO inventory loss.
--
-- That makes inventory double entry in the same sense the ledger is: the sum of
-- every quantity ever moved is zero, forever, and a stock report reconciles by
-- arithmetic rather than by a nightly job. This migration builds the vocabulary;
-- the moves themselves come next.
--
-- WHAT IS DELIBERATELY ABSENT
--
-- No foreign key into `master` or `books`. A supplier is a `master.parties` id
-- with no key behind it, and an account is a `books.accounts` id with none
-- either - see `account_mappings` below and ADR 0001. An app holding a key into
-- another app's schema is an app that can never be uninstalled.
--
-- Every reference to core IS qualified: `core.users`, never `users`. The search
-- path here is `inventory,public`, so an unqualified core reference fails loudly
-- rather than resolving by luck.

-- ---------------------------------------------------------------------------
-- Units of measure
-- ---------------------------------------------------------------------------
--
-- A unit belongs to a CLASS and carries a factor to that class's base unit.
-- Kilograms convert to grams because both measure weight; kilograms do not
-- convert to litres, and the class is what makes that refusal possible rather
-- than a rule somebody has to remember.

CREATE TABLE units (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Upper case, and unique that way. `kg` and `KG` are one unit.
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,

    -- count | weight | volume | length | area | time
    class       TEXT NOT NULL,

    -- How many of the class's base unit one of these is. NUMERIC(19,6) so a
    -- pound is 0.453592 kg exactly as written down.
    factor      NUMERIC(19, 6) NOT NULL,

    -- The one unit its class is measured against. Enforced unique below.
    is_base     BOOLEAN NOT NULL DEFAULT FALSE,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT units_code_present CHECK (char_length(code) BETWEEN 1 AND 12),
    CONSTRAINT units_code_upper CHECK (code = upper(code)),
    CONSTRAINT units_name_present CHECK (char_length(name) BETWEEN 1 AND 60),
    CONSTRAINT units_class_known CHECK (
        class IN ('count', 'weight', 'volume', 'length', 'area', 'time')
    ),
    -- A unit worth nothing of its base converts every quantity to nothing.
    CONSTRAINT units_factor_positive CHECK (factor > 0),
    -- The base unit is the one the factors are against, so its own is one.
    CONSTRAINT units_base_factor_is_one CHECK (NOT is_base OR factor = 1)
);

CREATE UNIQUE INDEX units_code ON units (code);

-- Exactly one base per class. Two would be two answers to "one what?".
CREATE UNIQUE INDEX units_one_base_per_class ON units (class) WHERE is_base;

-- ---------------------------------------------------------------------------
-- Locations
-- ---------------------------------------------------------------------------

CREATE TABLE locations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The full path as a person reads it: `WH/Stock/Zone A/Shelf 1`. Derived
    -- from the tree and STORED, because it is what every picker sees and
    -- rebuilding it per row in a grid is a query per row. Rewritten for the
    -- whole subtree when a node is renamed or moved.
    path        TEXT NOT NULL,

    -- The last segment only. `path` is `parent.path || '/' || name`.
    name        TEXT NOT NULL,

    parent_id   UUID REFERENCES locations (id) ON DELETE RESTRICT,

    -- internal | view | vendor | customer | inventory_loss | production | transit
    --
    -- Seven, and each one is the other side of a kind of entry. See
    -- `app_inventory::location` for why this is not two.
    kind        TEXT NOT NULL,

    -- Which building this belongs to. NULL for the counterpart locations,
    -- which belong to no building. The foreign key is added at the end of this
    -- file, once `warehouses` exists.
    warehouse_id UUID,

    -- A replenishment destination: a reordering rule may target it.
    is_replenished BOOLEAN NOT NULL DEFAULT FALSE,

    -- How often somebody should count what is here, in days. NULL for a
    -- location on no cycle.
    count_frequency_days INTEGER,

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT locations_name_present CHECK (char_length(name) BETWEEN 1 AND 120),
    -- `/` separates the path's segments, so a segment holding one would make a
    -- path nothing could read back.
    CONSTRAINT locations_name_no_separator CHECK (position('/' IN name) = 0),
    CONSTRAINT locations_path_present CHECK (char_length(path) BETWEEN 1 AND 1000),
    CONSTRAINT locations_kind_known CHECK (
        kind IN ('internal', 'view', 'vendor', 'customer',
                 'inventory_loss', 'production', 'transit')
    ),
    CONSTRAINT locations_not_own_parent CHECK (parent_id IS DISTINCT FROM id),
    -- Only somewhere stock actually sits can be a replenishment target.
    CONSTRAINT locations_replenish_is_internal CHECK (
        NOT is_replenished OR kind = 'internal'
    ),
    CONSTRAINT locations_count_frequency_positive CHECK (
        count_frequency_days IS NULL OR count_frequency_days >= 1
    ),
    -- A counterpart location belongs to no building; hanging one inside a
    -- warehouse would put in-transit stock in that warehouse's total.
    CONSTRAINT locations_counterparts_are_rootless CHECK (
        kind IN ('internal', 'view') OR parent_id IS NULL
    ),
    CONSTRAINT locations_counterparts_have_no_warehouse CHECK (
        kind IN ('internal', 'view') OR warehouse_id IS NULL
    )
);

CREATE UNIQUE INDEX locations_path ON locations (path);
CREATE INDEX locations_parent ON locations (parent_id);
CREATE INDEX locations_warehouse ON locations (warehouse_id) WHERE warehouse_id IS NOT NULL;
CREATE INDEX locations_kind ON locations (kind);

-- ---------------------------------------------------------------------------
-- Warehouses
-- ---------------------------------------------------------------------------
--
-- A warehouse is not a location; it OWNS several. Creating one creates a view
-- node named after it and a `Stock` location inside that, plus an `Input`, a
-- `Quality Control`, a `Packing Zone` or an `Output` where the workspace works
-- in more than one step.
--
-- That is what multi-step receiving is: not a flag that changes how a receipt
-- behaves, but a receipt into `WH/Input` followed by an ordinary internal move
-- to `WH/Stock`. Both are visible and the goods are findable in between.

CREATE TABLE warehouses (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Short, upper case, and the first segment of every location path inside
    -- it. No punctuation for that reason.
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,

    -- RESTRICT both: a warehouse whose own locations were deleted out from
    -- under it is a warehouse pointing at nothing.
    view_location_id  UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,
    stock_location_id UUID NOT NULL REFERENCES locations (id) ON DELETE RESTRICT,

    -- one_step | two_steps | three_steps
    receipt_steps  TEXT NOT NULL DEFAULT 'one_step',
    delivery_steps TEXT NOT NULL DEFAULT 'one_step',

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT warehouses_code_present CHECK (char_length(code) BETWEEN 1 AND 8),
    CONSTRAINT warehouses_code_upper CHECK (code = upper(code)),
    CONSTRAINT warehouses_code_alphanumeric CHECK (code ~ '^[A-Z0-9]+$'),
    CONSTRAINT warehouses_name_present CHECK (char_length(name) BETWEEN 1 AND 120),
    CONSTRAINT warehouses_receipt_steps_known CHECK (
        receipt_steps IN ('one_step', 'two_steps', 'three_steps')
    ),
    CONSTRAINT warehouses_delivery_steps_known CHECK (
        delivery_steps IN ('one_step', 'two_steps', 'three_steps')
    )
);

CREATE UNIQUE INDEX warehouses_code ON warehouses (code);

-- The other half of the cycle, added now that both tables exist.
ALTER TABLE locations
    ADD CONSTRAINT locations_warehouse_fk
    FOREIGN KEY (warehouse_id) REFERENCES warehouses (id) ON DELETE RESTRICT;

-- ---------------------------------------------------------------------------
-- Item categories
-- ---------------------------------------------------------------------------
--
-- Where costing, valuation and picking policy are decided - a decision about a
-- KIND of stock rather than about one item. A workspace that sets it per item
-- ends up with two items in one warehouse valued two ways and a stock account
-- nobody can tie out.

CREATE TABLE categories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `All/Raw materials/Fasteners`, on the same terms as a location's path.
    path        TEXT NOT NULL,
    name        TEXT NOT NULL,
    parent_id   UUID REFERENCES categories (id) ON DELETE RESTRICT,

    -- standard | average | fifo
    costing_method   TEXT NOT NULL DEFAULT 'average',
    -- manual | automated. Automated by default: a stock account that agrees
    -- with the stock report only at period end is wrong for most of the month.
    valuation        TEXT NOT NULL DEFAULT 'automated',
    -- fifo | lifo | fefo | closest_location
    removal_strategy TEXT NOT NULL DEFAULT 'fifo',

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT categories_name_present CHECK (char_length(name) BETWEEN 1 AND 120),
    CONSTRAINT categories_name_no_separator CHECK (position('/' IN name) = 0),
    CONSTRAINT categories_path_present CHECK (char_length(path) BETWEEN 1 AND 1000),
    CONSTRAINT categories_not_own_parent CHECK (parent_id IS DISTINCT FROM id),
    CONSTRAINT categories_costing_known CHECK (
        costing_method IN ('standard', 'average', 'fifo')
    ),
    CONSTRAINT categories_valuation_known CHECK (
        valuation IN ('manual', 'automated')
    ),
    CONSTRAINT categories_removal_known CHECK (
        removal_strategy IN ('fifo', 'lifo', 'fefo', 'closest_location')
    )
);

CREATE UNIQUE INDEX categories_path ON categories (path);
CREATE INDEX categories_parent ON categories (parent_id);

-- ---------------------------------------------------------------------------
-- Items
-- ---------------------------------------------------------------------------

CREATE TABLE items (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- GENERATED, out of `core.number_sequences`: `ITM-00042`. A code somebody
    -- invents is a code that collides with one somebody else invented last
    -- Tuesday. ADR 0006 section 3.
    code        TEXT NOT NULL,

    name        TEXT NOT NULL,

    -- TYPED, never generated. A UPC is printed on the packet by whoever made
    -- it, and generating one would be inventing a fact about the physical
    -- world. Unique where present; NULL is the usual case and many rows may
    -- have it.
    barcode     TEXT,

    description TEXT,

    -- goods | service
    kind        TEXT NOT NULL DEFAULT 'goods',

    -- Whether a quantity is kept. A flag rather than a third kind of item,
    -- because a workspace routinely decides partway through that it does want
    -- to count the screws after all, and that should be a tick.
    is_tracked  BOOLEAN NOT NULL DEFAULT TRUE,

    -- none | lot | serial
    tracking    TEXT NOT NULL DEFAULT 'none',
    uses_expiry BOOLEAN NOT NULL DEFAULT FALSE,

    category_id      UUID NOT NULL REFERENCES categories (id) ON DELETE RESTRICT,

    -- The unit every stored quantity is in. RESTRICT: deleting it would
    -- restate every quantity of this item that was ever recorded.
    stock_unit_id    UUID NOT NULL REFERENCES units (id) ON DELETE RESTRICT,
    -- What a supplier quotes in, where it differs. Converted on receipt.
    purchase_unit_id UUID NOT NULL REFERENCES units (id) ON DELETE RESTRICT,

    -- In the workspace's base currency, at Money's own four decimal places.
    -- The standard under `standard` costing; the running average or the latest
    -- layer's cost under the others.
    cost        NUMERIC(19, 4) NOT NULL DEFAULT 0,
    sale_price  NUMERIC(19, 4),

    can_be_purchased BOOLEAN NOT NULL DEFAULT TRUE,
    can_be_sold      BOOLEAN NOT NULL DEFAULT TRUE,

    weight_grams        BIGINT,
    purchase_lead_days  INTEGER,

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT items_code_present CHECK (char_length(code) BETWEEN 1 AND 40),
    CONSTRAINT items_code_shape CHECK (code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$'),
    CONSTRAINT items_name_present CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT items_barcode_shape CHECK (
        barcode IS NULL
        OR (char_length(barcode) BETWEEN 1 AND 64 AND barcode !~ '\s')
    ),
    CONSTRAINT items_description_length CHECK (
        description IS NULL OR char_length(description) <= 2000
    ),
    CONSTRAINT items_kind_known CHECK (kind IN ('goods', 'service')),
    CONSTRAINT items_tracking_known CHECK (tracking IN ('none', 'lot', 'serial')),
    -- A service has no quantity, ever.
    CONSTRAINT items_service_is_untracked CHECK (kind = 'goods' OR NOT is_tracked),
    -- Lot numbers on something nobody counts are lot numbers of nothing.
    CONSTRAINT items_tracking_needs_stock CHECK (is_tracked OR tracking = 'none'),
    -- An expiry date belongs to a lot; without one there is nothing to date,
    -- and FEFO would have nothing to sort by.
    CONSTRAINT items_expiry_needs_tracking CHECK (NOT uses_expiry OR tracking <> 'none'),
    CONSTRAINT items_cost_not_negative CHECK (cost >= 0),
    CONSTRAINT items_sale_price_not_negative CHECK (sale_price IS NULL OR sale_price >= 0),
    CONSTRAINT items_weight_not_negative CHECK (weight_grams IS NULL OR weight_grams >= 0),
    CONSTRAINT items_lead_days_not_negative CHECK (
        purchase_lead_days IS NULL OR purchase_lead_days >= 0
    )
);

CREATE UNIQUE INDEX items_code ON items (lower(code));
-- Partial, so the many items with no barcode do not collide with each other.
CREATE UNIQUE INDEX items_barcode ON items (barcode) WHERE barcode IS NOT NULL;
CREATE INDEX items_category ON items (category_id);
CREATE INDEX items_name ON items (lower(name));

-- ---------------------------------------------------------------------------
-- Account mapping
-- ---------------------------------------------------------------------------
--
-- WHY AN ACCOUNT ID MAY SIT IN THIS SCHEMA
--
-- It looks like Inventory holding a key into Books. It is not, and the
-- distinction is the one ADR 0001 draws: `account_id` is a BARE ID WITH NO
-- FOREIGN KEY, exactly as `books.invoices` carries a `master.parties` id.
-- Nothing here joins to `books.accounts`, the ledger verifies the id when a
-- posting arrives, and dropping the `books` schema leaves rows that resolve to
-- nothing rather than a database that will not drop.
--
-- `account_number` and `account_name` are a SNAPSHOT, so a mapping screen can
-- draw without a query per row. Refreshed when the mapping is edited, and never
-- trusted for anything but display.
--
-- WHY ONE TABLE RATHER THAN EIGHTEEN COLUMNS
--
-- Six roles on a category and six on an item is eighteen columns of
-- (id, number, name) that are empty on almost every row, and a seventh role
-- later is a migration on two tables. A row per override is empty by default,
-- which is what the default IS: nothing set anywhere, and every posting falls
-- through to `books.account_roles`.

CREATE TABLE account_mappings (
    -- 'category' | 'item'. Which table `owner_id` is in.
    --
    -- No foreign key, on purpose: one would have to be two nullable columns
    -- with a check that exactly one is set, and that is more machinery than a
    -- four-letter discriminator. The delete path clears these explicitly.
    owner_kind  TEXT NOT NULL,
    owner_id    UUID NOT NULL,

    -- A `phonix_ports::ledger::AccountRole`, spelled as it serialises.
    role        TEXT NOT NULL,

    account_id     UUID NOT NULL,
    account_number TEXT NOT NULL,
    account_name   TEXT NOT NULL,

    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    -- One answer per role per owner. Two would be a question with two answers.
    PRIMARY KEY (owner_kind, owner_id, role),

    CONSTRAINT account_mappings_owner_known CHECK (owner_kind IN ('category', 'item')),
    -- The roles an item or a category may speak for. Accounts payable is the
    -- supplier's, and landed cost, inventory adjustment and in-transit are
    -- workspace-wide policy that would be a different number per item for no
    -- reason anybody could explain the following March.
    CONSTRAINT account_mappings_role_known CHECK (
        role IN ('inventory', 'goods_received_not_invoiced',
                 'goods_delivered_not_invoiced', 'purchase_price_variance',
                 'revenue', 'cost_of_sales')
    ),
    CONSTRAINT account_mappings_number_present CHECK (
        char_length(account_number) BETWEEN 1 AND 40
    ),
    CONSTRAINT account_mappings_name_present CHECK (
        char_length(account_name) BETWEEN 1 AND 200
    )
);

-- "What else points at this account", asked before retiring one.
CREATE INDEX account_mappings_account ON account_mappings (account_id);
