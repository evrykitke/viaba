-- hr 0005: what somebody was expected to work.
--
-- Attendance says who was here and when. It cannot say whether they were late,
-- because nothing until now recorded what time they were due. Frappe HR keeps
-- the two apart for that reason and this follows it: the shift is the
-- expectation, the attendance record is the observation, and lateness is the
-- difference rather than a column either of them carries.
--
-- WHICH SHIFT SOMEBODY IS ON IS A DATED ASSIGNMENT, like their department,
-- their place and their calendar. Somebody moved from days to nights changed
-- shift on a date, and last month's punctuality has to be read against the
-- shift they were on last month.
--
-- TIMES ARE LOCAL CLOCK TIMES
--
-- `TIME` rather than `TIMESTAMPTZ`: a shift starts at nine in the morning where
-- the person works, and that is a different instant every time the clocks go
-- back. Turning one into an instant needs the workspace's zone and the date,
-- and it is done where both are known rather than frozen in here.

CREATE TABLE shift_types (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    code        TEXT NOT NULL,
    name        TEXT NOT NULL,

    starts_at   TIME NOT NULL,
    ends_at     TIME NOT NULL,

    -- Minutes after `starts_at` that still count as on time, and minutes
    -- before `ends_at` that still count as a full shift. Frappe HR has both
    -- and they are not the same number: five minutes late is traffic, five
    -- minutes early is a decision.
    late_grace_minutes       INTEGER NOT NULL DEFAULT 0,
    early_exit_grace_minutes INTEGER NOT NULL DEFAULT 0,

    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT shift_types_code_format CHECK (
        code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$' AND char_length(code) <= 40
    ),
    CONSTRAINT shift_types_name_present CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    ),
    -- A day is the longest a shift can be. No CHECK that `ends_at` is after
    -- `starts_at`: a night shift ends before it starts and is the ordinary
    -- case in half the industries that would use this.
    CONSTRAINT shift_types_late_grace_sane CHECK (
        late_grace_minutes BETWEEN 0 AND 1440
    ),
    CONSTRAINT shift_types_early_grace_sane CHECK (
        early_exit_grace_minutes BETWEEN 0 AND 1440
    )
);

CREATE UNIQUE INDEX shift_types_code_key ON shift_types (lower(code));
CREATE INDEX shift_types_active ON shift_types (name) WHERE is_active;

COMMENT ON TABLE shift_types IS
    'What a shift is: when it starts, when it ends, and how much lateness still counts as on time. Local clock times, not instants.';

-- Which shift somebody was on, for the span this assignment covers.
--
-- RESTRICT, like the department, role, place and calendar beside it: a shift
-- people are on is not one to delete.
ALTER TABLE assignments
    ADD COLUMN shift_type_id UUID REFERENCES shift_types (id) ON DELETE RESTRICT;

CREATE INDEX assignments_shift_type ON assignments (shift_type_id)
    WHERE shift_type_id IS NOT NULL;

-- `current_staff` carries every other assignment column and the screens read it
-- rather than the tables underneath. Replaced again, for the reason 0003 gave.
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
       h.name                     AS holiday_list_name,
       a.shift_type_id,
       s.name                     AS shift_type_name
  FROM employees e
  JOIN engagements g ON g.employee_id = e.id AND g.ended_on IS NULL
  LEFT JOIN assignments a ON a.engagement_id = g.id AND a.effective_to IS NULL
  LEFT JOIN departments d ON d.id = a.department_id
  LEFT JOIN job_positions j ON j.id = a.job_position_id
  LEFT JOIN work_locations w ON w.id = a.work_location_id
  LEFT JOIN employees m ON m.id = a.manager_id
  LEFT JOIN holiday_lists h ON h.id = a.holiday_list_id
  LEFT JOIN shift_types s ON s.id = a.shift_type_id;
