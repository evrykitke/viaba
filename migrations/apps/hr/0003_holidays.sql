-- hr 0003: the days nobody is expected to work.
--
-- WHAT THIS ADDS
--
--   holiday_lists  a named calendar covering a span, usually a region's year
--   holidays       one dated day off on one list
--
-- A list per region rather than a flag per date: two offices in two countries
-- share a workspace and not a calendar, and "is this a working day" has no
-- answer until somebody says whose.
--
-- WHICH LIST APPLIES IS A DATED ASSIGNMENT, not a column on `employees`. A
-- person who moves office changes calendar on a date, and the previous year's
-- attendance still has to be read against the calendar that was in force then.
-- That is the rule the whole of 0002 was written for.
--
-- WEEKENDS ARE ROWS
--
-- Frappe HR generates weekly offs into the list rather than deriving them, and
-- this follows it. A derived weekend needs a rule per region, and the rule is
-- not "Saturday and Sunday" in much of the world - so the generated rows are
-- the record, and changing next year's pattern does not rewrite last year's.

CREATE TABLE holiday_lists (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    code        TEXT NOT NULL,
    name        TEXT NOT NULL,

    -- The span the list is an answer for. Outside it the list says nothing,
    -- which is not the same as saying "working day": a calendar that silently
    -- covers every date would make a missing year look like a full one.
    valid_from  DATE NOT NULL,
    valid_to    DATE NOT NULL,

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT holiday_lists_code_format CHECK (
        code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$' AND char_length(code) <= 40
    ),
    CONSTRAINT holiday_lists_name_present CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    ),
    CONSTRAINT holiday_lists_span_forwards CHECK (valid_to >= valid_from)
);

CREATE UNIQUE INDEX holiday_lists_code_key ON holiday_lists (lower(code));
CREATE INDEX holiday_lists_active ON holiday_lists (valid_from) WHERE is_active;

CREATE TABLE holidays (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    holiday_list_id UUID NOT NULL REFERENCES holiday_lists (id) ON DELETE CASCADE,

    observed_on     DATE NOT NULL,

    -- What it is called where it is observed. 'Saturday' for a generated
    -- weekly off, which is why this is not nullable.
    name            TEXT NOT NULL,

    -- Whether it came from the weekly pattern rather than from somebody naming
    -- a day. Only so a screen can offer to replace the generated ones without
    -- touching the named ones.
    is_weekly_off   BOOLEAN NOT NULL DEFAULT FALSE,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT holidays_name_present CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    )
);

-- One day is off once. A list naming it twice would count a holiday twice in
-- every figure derived from the list.
CREATE UNIQUE INDEX holidays_day_key ON holidays (holiday_list_id, observed_on);
CREATE INDEX holidays_by_date ON holidays (observed_on);

-- Which calendar somebody is on, for the span this assignment covers.
--
-- RESTRICT, like the other three: a list people are assigned to is not one to
-- delete. NULL means nobody has said, and the caller decides what to do about
-- that rather than being handed a default that is wrong somewhere.
ALTER TABLE assignments
    ADD COLUMN holiday_list_id UUID REFERENCES holiday_lists (id) ON DELETE RESTRICT;

CREATE INDEX assignments_holiday_list ON assignments (holiday_list_id)
    WHERE holiday_list_id IS NOT NULL;

-- `current_staff` carries every other assignment column, and the screens read
-- it rather than the three tables underneath. Replaced rather than left behind:
-- a view that knows about department, role and place but not about which
-- calendar somebody is on sends the next query back to the joins it exists to
-- save. The new column goes last, which is what CREATE OR REPLACE allows.
CREATE OR REPLACE VIEW current_staff AS
SELECT e.id                       AS employee_id,
       e.code,
       e.given_name,
       e.family_name,
       e.preferred_name,
       e.work_email,
       e.work_phone,
       e.user_id,
       g.id                       AS engagement_id,
       g.started_on,
       g.employment_type,
       g.expected_end_on,
       a.id                       AS assignment_id,
       a.effective_from           AS assigned_from,
       a.department_id,
       d.name                     AS department_name,
       d.is_cost_centre,
       a.job_position_id,
       j.title                    AS job_title,
       a.work_location_id,
       w.name                     AS work_location_name,
       a.manager_id,
       m.given_name               AS manager_given_name,
       m.family_name              AS manager_family_name,
       a.holiday_list_id,
       h.name                     AS holiday_list_name
  FROM employees e
  JOIN engagements g ON g.employee_id = e.id AND g.ended_on IS NULL
  LEFT JOIN assignments a ON a.engagement_id = g.id AND a.effective_to IS NULL
  LEFT JOIN departments d ON d.id = a.department_id
  LEFT JOIN job_positions j ON j.id = a.job_position_id
  LEFT JOIN work_locations w ON w.id = a.work_location_id
  LEFT JOIN employees m ON m.id = a.manager_id
  LEFT JOIN holiday_lists h ON h.id = a.holiday_list_id;
