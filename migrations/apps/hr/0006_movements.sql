-- hr 0006: the documents behind the dated chain.
--
-- `assignments` and `engagements` record what became true. Nothing recorded the
-- act that made it true: who decided, when they decided it, and why. A
-- promotion today is a hand-written assignment row, and the only trace of the
-- decision is an audit entry saying a row changed.
--
-- So a movement is a document. It is written as a draft, confirmed, and the
-- confirmation is what writes the assignment or closes the engagement - through
-- the same service functions a hand-written move already uses, so there remains
-- exactly one way those rows are made.
--
-- ONE TABLE, THREE KINDS
--
-- Promotion and transfer are the same document with a different word on it:
-- both end one assignment and open the next. An exit is the third kind rather
-- than a table of its own for the reason `invoices.kind` holds a credit note -
-- a second table would be this machinery written twice, and every screen that
-- lists "what happened to this person" would read both.
--
-- Onboarding is deliberately NOT a fourth kind. Hiring already has a document:
-- the employee form opens the engagement and the first assignment together, and
-- a movement recording the arrival of somebody who does not exist yet would
-- have nobody to point at.

CREATE TABLE movements (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- NULL while a draft. Taken from the sequence at confirm, exactly as an
    -- invoice takes its number at post: a discarded draft leaves no gap.
    number        TEXT,

    -- promotion | transfer | exit
    kind          TEXT NOT NULL,

    -- draft | confirmed | cancelled
    status        TEXT NOT NULL DEFAULT 'draft',

    employee_id   UUID NOT NULL REFERENCES employees (id) ON DELETE CASCADE,

    -- The day the change takes effect, which is the assignment's
    -- `effective_from` or the engagement's `ended_on`.
    effective_on  DATE NOT NULL,

    -- What a promotion or transfer moves them to: the shape of an assignment,
    -- held on the document so that a draft can be reviewed before it is true.
    -- All nullable, because an assignment's own columns are.
    department_id    UUID REFERENCES departments (id) ON DELETE RESTRICT,
    job_position_id  UUID REFERENCES job_positions (id) ON DELETE RESTRICT,
    work_location_id UUID REFERENCES work_locations (id) ON DELETE RESTRICT,
    manager_id       UUID REFERENCES employees (id) ON DELETE SET NULL,
    holiday_list_id  UUID REFERENCES holiday_lists (id) ON DELETE RESTRICT,
    shift_type_id    UUID REFERENCES shift_types (id) ON DELETE RESTRICT,

    -- What an exit records, from the list `EndReason` already defines.
    end_reason    TEXT,

    -- Why, in the decider's words. Free text for the reason `assignments.reason`
    -- is: "covering maternity leave" is not an enum anybody would have guessed.
    reason        TEXT,

    -- The assignment this document opened, where it opened one. So the document
    -- can point at its own effect rather than leaving a reader to infer it from
    -- the dates.
    assignment_id UUID REFERENCES assignments (id) ON DELETE SET NULL,

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by    UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID REFERENCES core.users (id) ON DELETE SET NULL,
    confirmed_at  TIMESTAMPTZ,
    confirmed_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT movements_kind_known CHECK (kind IN ('promotion', 'transfer', 'exit')),
    CONSTRAINT movements_status_known CHECK (
        status IN ('draft', 'confirmed', 'cancelled')
    ),

    -- An exit says why; a promotion does not, because the reason it carries is
    -- the free-text one and `end_reason` is the closed list.
    CONSTRAINT movements_exit_has_a_reason CHECK (
        (kind = 'exit') = (end_reason IS NOT NULL)
    ),

    -- An exit opens no assignment, so it names none of the columns one would
    -- need. Row-local, so it belongs here.
    CONSTRAINT movements_exit_moves_nobody CHECK (
        kind <> 'exit'
        OR (department_id IS NULL AND job_position_id IS NULL
            AND work_location_id IS NULL AND manager_id IS NULL
            AND holiday_list_id IS NULL AND shift_type_id IS NULL)
    ),

    -- A number is what confirming takes, so the two arrive together.
    CONSTRAINT movements_confirmed_is_numbered CHECK (
        (status = 'confirmed') = (number IS NOT NULL)
    ),
    CONSTRAINT movements_confirmed_is_stamped CHECK (
        (status = 'confirmed') = (confirmed_at IS NOT NULL)
    ),

    CONSTRAINT movements_reason_length CHECK (
        reason IS NULL OR char_length(reason) <= 500
    )
);

CREATE UNIQUE INDEX movements_number_key ON movements (number)
    WHERE number IS NOT NULL;

-- What a personnel file reads: everything that happened to one person, newest
-- first.
CREATE INDEX movements_person ON movements (employee_id, effective_on DESC);

-- What the list screen reads when nobody has picked a person.
CREATE INDEX movements_pending ON movements (effective_on DESC)
    WHERE status = 'draft';

COMMENT ON TABLE movements IS
    'The document behind a promotion, transfer or exit: who decided, when, and why. Confirming it is what writes the assignment or closes the engagement.';
