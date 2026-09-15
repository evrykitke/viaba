-- hr 0004: what was recorded, as opposed to what was expected.
--
-- One row per person per day. Frappe HR keeps attendance as its own record
-- rather than as a side-effect of a shift, and every other HR number is derived
-- from it, so it is the input rather than a report.
--
-- WHAT IS NOT HERE
--
-- No coordinates. Frappe HR's mobile app sends them with each check-in; viaba
-- does not, decided 2026-09-15. Attendance says who was here and who says so,
-- which is what a payroll run needs; where somebody was standing is a different
-- product with a consent story attached.
--
-- No leave status either. Leave is deliberately not built - ADR 0006 section 9
-- - and a status enum that offers it would be a promise this schema cannot
-- keep. A day somebody was away on agreed leave is `absent` with a note until
-- there is a leave record to point at.
--
-- ABSENT IS RECORDED, NOT INFERRED
--
-- A missing row is not an absence. It is a day nobody keyed anything for, which
-- is the ordinary state of every future date and of every day before this table
-- existed. Absence is a row somebody wrote, and the difference is the whole
-- reason `status` is not a boolean.

CREATE TABLE attendance (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- CASCADE, like engagements: deleting an employee is only ever allowed for
    -- a row created in error, and attendance for somebody who does not exist is
    -- not a thing to keep.
    employee_id  UUID NOT NULL REFERENCES employees (id) ON DELETE CASCADE,

    on_date      DATE NOT NULL,

    -- present | half_day | absent
    status       TEXT NOT NULL,

    -- The clock times, where anybody recorded them. Optional because a manager
    -- marking a week present after the fact has the day but not the minutes,
    -- and a day with no times is still a day that was worked.
    checked_in_at  TIMESTAMPTZ,
    checked_out_at TIMESTAMPTZ,

    -- device | manual | import
    --
    -- Who or what asserted this. A figure somebody will be paid on should say
    -- whether a clock recorded it or a manager typed it, and the two are not
    -- equally good evidence.
    source       TEXT NOT NULL DEFAULT 'manual',

    note         TEXT,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT attendance_status_known CHECK (
        status IN ('present', 'half_day', 'absent')
    ),
    CONSTRAINT attendance_source_known CHECK (
        source IN ('device', 'manual', 'import')
    ),
    CONSTRAINT attendance_out_after_in CHECK (
        checked_in_at IS NULL
        OR checked_out_at IS NULL
        OR checked_out_at >= checked_in_at
    ),
    CONSTRAINT attendance_note_length CHECK (
        note IS NULL OR char_length(note) <= 500
    )
);

-- One day is recorded once. Two rows for one person on one date is two answers
-- to a question with one, and every figure derived from this would double it.
CREATE UNIQUE INDEX attendance_day_key ON attendance (employee_id, on_date);

-- What a month's timesheet reads, and what a payroll run would.
CREATE INDEX attendance_by_date ON attendance (on_date);

COMMENT ON TABLE attendance IS
    'One row per person per day: what was recorded, and who says so. A missing row is a day nobody keyed, not an absence.';
