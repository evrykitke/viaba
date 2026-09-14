-- ---------------------------------------------------------------------------
-- 0022: attachments - the paperwork a record came with.
--
-- A goods receipt arrives with a delivery note, a bill with a supplier's PDF,
-- an employee with a signed contract. None of those are fields; they are the
-- evidence behind the fields, and until now there was nowhere to put them.
--
-- # One table for every kind of record
--
-- The alternative is a join table per document - receipt_files, bill_files,
-- employee_files - which is the same four columns written out once per screen
-- that ever wants an attachment, and a migration every time another one does.
-- This is the shape `entity_events` already uses for exactly the same reason,
-- and the addressing is deliberately identical: `entity_type` is the name of a
-- `phonix_core::audit::EntityKind` and `entity_id` is that record's id as text.
-- A screen that can show a history can show its attachments with the same two
-- values, and Odoo's `ir.attachment` reaches the same answer from the same
-- pressure.
--
-- # What that costs, and what is done about it
--
-- `entity_id` cannot be a foreign key: it names a row in whichever table
-- `entity_type` picks out, and no constraint can follow that. So deleting a
-- draft receipt would leave its attachments behind, pointing at nothing -
-- which in Odoo is exactly what happens, and why an `ir.attachment` table
-- accumulates rows nobody can name.
--
-- The answer here is that detaching is part of deleting: a service that
-- removes a record removes its attachments in the same transaction. That is a
-- rule in code rather than in the schema, which is the deliberate trade - the
-- alternative buys referential integrity for one entity at the price of not
-- being able to attach anything to the next one.
--
-- # RESTRICT on the file, not CASCADE
--
-- The same choice `item_images` made. A stored file deleted out from under an
-- attachment would leave a row offering a download that fails; the files
-- screen refuses the delete and says what is using it instead.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS core.attachments (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Which kind of record, e.g. 'goods_receipt', 'bill', 'employee'.
    -- Unconstrained for the reason `entity_events.entity_type` is: the
    -- vocabulary lives in `phonix_core::audit::EntityKind`, and a CHECK here
    -- would make attaching a file to a new entity a migration.
    entity_type  TEXT NOT NULL,

    -- Which record, as text rather than as a UUID, so a singleton or a record
    -- keyed by a pair has somewhere to go. Same as `entity_events.entity_id`.
    entity_id    TEXT NOT NULL,

    file_id      UUID NOT NULL REFERENCES core.file_uploads (id) ON DELETE RESTRICT,

    -- What this document is, in the filer's words: 'Supplier invoice',
    -- 'Signed delivery note'. NULL means the file's own name is the caption,
    -- which is right often enough to be the default.
    title        TEXT,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT attachments_entity_type_present CHECK (char_length(entity_type) BETWEEN 1 AND 64),
    CONSTRAINT attachments_entity_id_present CHECK (char_length(entity_id) BETWEEN 1 AND 64),
    CONSTRAINT attachments_title_length CHECK (
        title IS NULL OR char_length(title) BETWEEN 1 AND 200
    )
);

-- The same file attached to the same record twice is a list showing one
-- document twice, and a delete that only removes half of it.
CREATE UNIQUE INDEX IF NOT EXISTS attachments_record_file
    ON core.attachments (entity_type, entity_id, file_id);

-- What a document screen reads: everything on this record, newest first.
CREATE INDEX IF NOT EXISTS attachments_record
    ON core.attachments (entity_type, entity_id, created_at DESC);

-- What the files screen reads to answer "can this be deleted, and if not, why".
CREATE INDEX IF NOT EXISTS attachments_file
    ON core.attachments (file_id);
