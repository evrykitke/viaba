-- ---------------------------------------------------------------------------
-- 0024: what a tenant keeps about each kind of document.
--
-- Answers, never a layout. The bands are code; see docs/adr/0008-reporting.md.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS core.document_settings (
    -- 'invoice', 'receipt'. Unconstrained for the reason
    -- `attachments.entity_type` is: the vocabulary is `config/numbering/`'s,
    -- and a CHECK here would make issuing a new document a migration.
    document_type   TEXT PRIMARY KEY,

    theme           TEXT NOT NULL,
    paper           TEXT NOT NULL,
    orientation     TEXT NOT NULL,

    -- All three together, or none of them: no mark is NULL rather than a
    -- placement nothing draws.
    logo_band       TEXT,
    logo_align      TEXT,
    logo_height_mm  REAL,

    -- The tenant's own words, stored as words. Not an i18n key.
    header_text     TEXT,
    footer_text     TEXT,

    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT document_settings_type_present CHECK (
        char_length(document_type) BETWEEN 1 AND 64
    ),
    -- A free-text theme is a report designer arriving through the back door.
    CONSTRAINT document_settings_theme_known CHECK (
        theme IN ('modern', 'compact', 'professional')
    ),
    CONSTRAINT document_settings_paper_known CHECK (
        paper IN ('a4', 'a5', 'letter', 'legal')
    ),
    CONSTRAINT document_settings_orientation_known CHECK (
        orientation IN ('portrait', 'landscape')
    ),
    CONSTRAINT document_settings_logo_band_known CHECK (
        logo_band IS NULL OR logo_band IN ('report_header', 'page_header')
    ),
    CONSTRAINT document_settings_logo_align_known CHECK (
        logo_align IS NULL OR logo_align IN ('start', 'center', 'end')
    ),
    CONSTRAINT document_settings_logo_whole CHECK (
        (logo_band IS NULL) = (logo_align IS NULL)
        AND (logo_band IS NULL) = (logo_height_mm IS NULL)
    ),
    CONSTRAINT document_settings_logo_height_sane CHECK (
        logo_height_mm IS NULL OR (logo_height_mm > 0 AND logo_height_mm <= 100)
    ),
    CONSTRAINT document_settings_header_length CHECK (
        header_text IS NULL OR char_length(header_text) BETWEEN 1 AND 500
    ),
    CONSTRAINT document_settings_footer_length CHECK (
        footer_text IS NULL OR char_length(footer_text) BETWEEN 1 AND 500
    )
);
