-- inventory 0002: variants, and pictures of them.
--
-- THE DECISION THIS FILE EXISTS TO MAKE EARLY
--
-- **Stock is held against a VARIANT, never against an item.** A blue medium
-- t-shirt and a red large one are different things on a shelf, sold at
-- different rates, counted separately and possibly costed differently.
--
-- Adding variants later would mean restating every movement ever recorded, so
-- the table arrives now even though the screens for it come afterwards, and
-- every stock table built on top of this references `item_variants (id)`.
--
-- **Every item has at least one variant.** An item that varies by nothing has
-- exactly one, flagged `is_default`, whose combination is empty. That is what
-- lets everything downstream speak only of variants: there is no second code
-- path for "an item without variants", because there is no such thing.
--
-- THE SHAPE
--
--   attributes            Colour, Size, Material
--   attribute_values      Red, Blue; S, M, L
--   item_attribute_values which of those values THIS item is offered in
--   item_variants         one row per combination actually sold
--   variant_values        the combination that row stands for
--
-- Generating the cross product is the service's job and it is deliberately not
-- automatic on every save: an item offered in 6 colours, 5 sizes and 3
-- materials is 90 variants, and a workspace that meant to add one colour should
-- be told that before 90 rows appear.

-- ---------------------------------------------------------------------------
-- Attributes
-- ---------------------------------------------------------------------------

CREATE TABLE attributes (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,

    -- How a picker draws it: `select` for a list, `radio` for a few, `colour`
    -- for swatches. Presentation, and it lives with the data because the same
    -- attribute is drawn the same way on every screen and in a POS that was
    -- not written yet.
    display     TEXT NOT NULL DEFAULT 'select',

    -- Display order in a form, low first.
    position    INTEGER NOT NULL DEFAULT 0,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT attributes_name_present CHECK (char_length(name) BETWEEN 1 AND 60),
    CONSTRAINT attributes_display_known CHECK (display IN ('select', 'radio', 'colour'))
);

CREATE UNIQUE INDEX attributes_name ON attributes (lower(name));

CREATE TABLE attribute_values (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attribute_id UUID NOT NULL REFERENCES attributes (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,

    -- `#c0392b`, for a swatch. NULL for everything that is not a colour.
    swatch       TEXT,

    position     INTEGER NOT NULL DEFAULT 0,
    is_active    BOOLEAN NOT NULL DEFAULT TRUE,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT attribute_values_name_present CHECK (char_length(name) BETWEEN 1 AND 60),
    CONSTRAINT attribute_values_swatch_shape CHECK (
        swatch IS NULL OR swatch ~ '^#[0-9a-fA-F]{6}$'
    )
);

-- "Red" once per attribute. Two would be two variants nobody could tell apart.
CREATE UNIQUE INDEX attribute_values_unique ON attribute_values (attribute_id, lower(name));
CREATE INDEX attribute_values_attribute ON attribute_values (attribute_id);

-- ---------------------------------------------------------------------------
-- Which values an item is offered in
-- ---------------------------------------------------------------------------
--
-- Not "which attributes" - which VALUES. A shirt that comes in red and blue but
-- not green is three facts, and storing only "this shirt varies by colour"
-- would offer every colour the workspace has ever used.

CREATE TABLE item_attribute_values (
    item_id      UUID NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    attribute_id UUID NOT NULL REFERENCES attributes (id) ON DELETE RESTRICT,
    value_id     UUID NOT NULL REFERENCES attribute_values (id) ON DELETE RESTRICT,

    PRIMARY KEY (item_id, value_id)
);

CREATE INDEX item_attribute_values_item ON item_attribute_values (item_id, attribute_id);

-- ---------------------------------------------------------------------------
-- Variants
-- ---------------------------------------------------------------------------

CREATE TABLE item_variants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id     UUID NOT NULL REFERENCES items (id) ON DELETE CASCADE,

    -- The item's code with the combination appended: `ITM-00042-RED-M`.
    -- Generated like the item's own, and unique across the workspace because it
    -- is what a picking list and a barcode label print.
    code        TEXT NOT NULL,

    -- Its own UPC. Typed, never generated - a red medium shirt has a different
    -- barcode on the packet from a blue large one, and that is the single
    -- commonest reason a workspace needs variants at all.
    barcode     TEXT,

    -- What this combination adds to the item's price and cost. Usually zero;
    -- a larger size that costs more to make is why it is not always.
    price_extra NUMERIC(19, 4) NOT NULL DEFAULT 0,
    cost_extra  NUMERIC(19, 4) NOT NULL DEFAULT 0,

    -- The one variant of an item that varies by nothing, and the one a
    -- document defaults to. Exactly one per item, enforced below.
    is_default  BOOLEAN NOT NULL DEFAULT FALSE,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT item_variants_code_present CHECK (char_length(code) BETWEEN 1 AND 60),
    CONSTRAINT item_variants_barcode_shape CHECK (
        barcode IS NULL
        OR (char_length(barcode) BETWEEN 1 AND 64 AND barcode !~ '\s')
    )
);

CREATE UNIQUE INDEX item_variants_code ON item_variants (lower(code));
CREATE UNIQUE INDEX item_variants_barcode ON item_variants (barcode) WHERE barcode IS NOT NULL;
CREATE INDEX item_variants_item ON item_variants (item_id);

-- One default per item. The row every document falls back to.
CREATE UNIQUE INDEX item_variants_one_default ON item_variants (item_id) WHERE is_default;

CREATE TABLE variant_values (
    variant_id   UUID NOT NULL REFERENCES item_variants (id) ON DELETE CASCADE,
    attribute_id UUID NOT NULL REFERENCES attributes (id) ON DELETE RESTRICT,
    value_id     UUID NOT NULL REFERENCES attribute_values (id) ON DELETE RESTRICT,

    -- One value per attribute per variant: a shirt is not red AND blue.
    PRIMARY KEY (variant_id, attribute_id)
);

CREATE INDEX variant_values_value ON variant_values (value_id);

-- ---------------------------------------------------------------------------
-- Pictures
-- ---------------------------------------------------------------------------
--
-- A point-of-sale screen is a grid of pictures. Somebody serving a queue picks
-- by sight, and a POS whose tiles all say "ITM-00042" is slower than the till
-- it replaced - so this is not decoration, and it is why the table is here
-- before the POS is.
--
-- An image belongs to an item, and optionally to one variant of it: the red
-- shirt gets its own photograph, and everything else falls back to the item's.
--
-- `file_id` points into `core.file_uploads`, which is core rather than another
-- app, so
-- this one IS a real foreign key. RESTRICT rather than CASCADE: deleting a
-- stored file out from under a product image would leave a tile that renders
-- nothing, and the file screen should say what is using it instead.

CREATE TABLE item_images (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id     UUID NOT NULL REFERENCES items (id) ON DELETE CASCADE,

    -- NULL means "the item's own picture", used by every variant that has none.
    variant_id  UUID REFERENCES item_variants (id) ON DELETE CASCADE,

    file_id     UUID NOT NULL REFERENCES core.file_uploads (id) ON DELETE RESTRICT,

    -- What a screen reader says, and what a printed catalogue captions.
    alt_text    TEXT,

    -- Gallery order, low first.
    position    INTEGER NOT NULL DEFAULT 0,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT item_images_alt_length CHECK (
        alt_text IS NULL OR char_length(alt_text) <= 200
    )
);

-- The same photograph once per item, and once per variant. Two rows for one
-- file is a gallery showing the same picture twice.
CREATE UNIQUE INDEX item_images_item_file
    ON item_images (item_id, file_id) WHERE variant_id IS NULL;
CREATE UNIQUE INDEX item_images_variant_file
    ON item_images (variant_id, file_id) WHERE variant_id IS NOT NULL;

CREATE INDEX item_images_item ON item_images (item_id, position);
CREATE INDEX item_images_variant ON item_images (variant_id, position)
    WHERE variant_id IS NOT NULL;
CREATE INDEX item_images_file ON item_images (file_id);

-- ---------------------------------------------------------------------------
-- Every item that exists already gets its one default variant
-- ---------------------------------------------------------------------------
--
-- Empty on a new database and not on an upgraded one. Written anyway, because
-- the invariant "every item has a variant" has to be true of every row from the
-- moment this file runs, not only of rows created afterwards.

INSERT INTO item_variants (item_id, code, barcode, is_default)
SELECT i.id, i.code, i.barcode, TRUE
  FROM items i
 WHERE NOT EXISTS (SELECT 1 FROM item_variants v WHERE v.item_id = i.id);
