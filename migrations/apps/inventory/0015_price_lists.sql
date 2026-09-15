-- inventory 0015: what a thing costs, and to whom.
--
-- Until now there was one `items.sale_price` and the sales order opened every
-- line on it. That is one number for a wholesale customer and a walk-in, which
-- is the gap ERPNext fills with Price List plus Item Price and Odoo with
-- pricelists.
--
--   price_lists   a named set of prices, in one currency
--   item_prices   what one variant costs in one list, above a quantity,
--                 between two dates
--
-- PER VARIANT, AND PER STOCK UNIT
--
-- Per variant because stock hangs off a variant everywhere else here, and a
-- price for "the item" would be a price for something nobody sells. Per stock
-- unit because `items.sale_price` already is - there is no sales unit on an
-- item, only a stock one and a purchase one - so this changes where a price
-- comes from without changing what it means.
--
-- WHY THE WINDOW IS TWO NULLABLE DATES
--
-- A price with no `valid_from` has always applied and one with no `valid_to`
-- still does. That is the ordinary case and it must not need a date somebody
-- invents: a list keyed on made-up dates is one nobody trusts to be current.

CREATE TABLE price_lists (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code          TEXT NOT NULL,
    name          TEXT NOT NULL,

    -- A list prices in one currency. A customer on a USD list is quoted in USD
    -- whatever the workspace keeps its books in.
    currency_code TEXT NOT NULL,

    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT price_lists_code_key UNIQUE (code),
    CONSTRAINT price_lists_code_not_blank CHECK (btrim(code) <> ''),
    CONSTRAINT price_lists_name_not_blank CHECK (btrim(name) <> '')
);

COMMENT ON TABLE price_lists IS
    'A named set of selling prices in one currency. ERPNext''s Price List.';

CREATE TABLE item_prices (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    price_list_id UUID NOT NULL REFERENCES price_lists (id) ON DELETE CASCADE,
    variant_id    UUID NOT NULL REFERENCES item_variants (id) ON DELETE CASCADE,

    -- The break this price applies from, in the item's stock unit. Zero is
    -- "any quantity", which is what a list without breaks is made of.
    min_quantity  NUMERIC(19, 6) NOT NULL DEFAULT 0,

    -- Open at either end. See the header.
    valid_from    DATE,
    valid_to      DATE,

    -- Per stock unit, in the list's currency.
    unit_price    NUMERIC(19, 4) NOT NULL,

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT item_prices_quantity_not_negative CHECK (min_quantity >= 0),
    CONSTRAINT item_prices_price_not_negative CHECK (unit_price >= 0),
    CONSTRAINT item_prices_window_ordered
        CHECK (valid_from IS NULL OR valid_to IS NULL OR valid_to >= valid_from)
);

COMMENT ON TABLE item_prices IS
    'What one variant costs in one list, above a quantity, between two dates. ERPNext''s Item Price.';

-- The lookup every quotation makes: one list, one variant, cheapest read.
-- Overlapping rows are allowed on purpose - a quantity break and a dated offer
-- are both ordinary, and which one wins is a rule in code rather than a
-- constraint that would forbid stating both.
CREATE INDEX item_prices_lookup ON item_prices (price_list_id, variant_id, min_quantity DESC);
