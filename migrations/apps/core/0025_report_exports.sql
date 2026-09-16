-- ---------------------------------------------------------------------------
-- 0025: the exports somebody asked for.
--
-- An unbounded export is a job; a bounded one renders in the request and never
-- reaches this table. See docs/adr/0008-reporting.md.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS core.report_exports (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The definition's own id: 'customer-statement', 'product-list'.
    report_id     TEXT NOT NULL,

    -- What it was run with - the customer, the span. Opaque here; the report
    -- is what reads it back.
    parameters    JSONB NOT NULL DEFAULT '{}'::jsonb,

    format        TEXT NOT NULL,
    state         TEXT NOT NULL DEFAULT 'requested',

    -- Who asked. A worker has no caller of its own and renders as them, so
    -- this is part of the row rather than a detail of the request. NULL once
    -- that account is gone, which stops the render rather than anonymising it.
    requested_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    requested_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- When a worker took it, so one that died mid-job can be swept.
    claimed_at    TIMESTAMPTZ,
    finished_at   TIMESTAMPTZ,

    -- RESTRICT, like every other reference to a stored file: a row offering a
    -- download that fails is worse than a delete that refuses.
    file_id       UUID REFERENCES core.file_uploads (id) ON DELETE RESTRICT,
    failure       TEXT,

    CONSTRAINT report_exports_report_present CHECK (
        char_length(report_id) BETWEEN 1 AND 64
    ),
    CONSTRAINT report_exports_format_known CHECK (format IN ('csv', 'xlsx', 'pdf')),
    CONSTRAINT report_exports_state_known CHECK (
        state IN ('requested', 'running', 'ready', 'failed')
    ),
    CONSTRAINT report_exports_ready_has_a_file CHECK (
        state <> 'ready' OR file_id IS NOT NULL
    ),
    CONSTRAINT report_exports_failure_has_a_reason CHECK (
        state <> 'failed' OR failure IS NOT NULL
    ),
    CONSTRAINT report_exports_failure_length CHECK (
        failure IS NULL OR char_length(failure) BETWEEN 1 AND 500
    )
);

-- What the exporter claims from: the oldest thing still waiting.
CREATE INDEX IF NOT EXISTS report_exports_waiting
    ON core.report_exports (state, requested_at)
    WHERE state IN ('requested', 'running');

-- What a person's own exports are, newest first.
CREATE INDEX IF NOT EXISTS report_exports_by_requester
    ON core.report_exports (requested_by, requested_at DESC);
