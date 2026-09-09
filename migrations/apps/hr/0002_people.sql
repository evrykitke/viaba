-- hr 0002: the people, and the fact that what they do changes.
--
-- WHAT THIS ADDS
--
--   job_positions   a role, which exists whether or not anybody holds it
--   work_locations  where somebody works
--   employees       the person
--   engagements     one period of employment; a rehire is a second one
--   assignments     what somebody was doing, between two dates
--
-- WHY THERE ARE FIVE TABLES AND NOT ONE
--
-- The single most common failure in HR systems is recorded plainly in the
-- migration literature: teams carry the *current state* - job title,
-- department, manager - as columns on the employee row, and then the system
-- shows the correct current title with no record of when it changed or what it
-- was before. Tenure analysis, compensation benchmarking and every report that
-- asks "what did this department cost last year" break at once, and they break
-- silently, because the current answer is still right.
--
-- So `employees` holds only what does not change when somebody is promoted, and
-- everything that does is a dated `assignments` row. "Which department was this
-- person in last March" is a query here rather than a thing nobody kept.
--
-- That matters to this codebase specifically. A requisition snapshots the cost
-- centre it was charged to (ADR 0006 section 9), which answers the question for
-- documents already written - but nothing answers it for a person, and
-- "reassign the department and re-run the report" is exactly the restatement
-- the snapshot exists to prevent.
--
-- WHY AN ENGAGEMENT IS NOT AN ASSIGNMENT
--
-- They are different facts and systems that fuse them get rehires wrong. An
-- engagement is a period of employment: hired on a date, ended on a date, for a
-- reason. An assignment is what somebody was doing during part of one.
--
-- A person who leaves and comes back is the "duplicate identity" case the
-- identity-management literature names: systems that model employment as a flag
-- on the person either lose the first stint or create a second person, and a
-- second person means two national insurance numbers for one human being and a
-- tenure figure that starts again from zero. Here it is a second engagement
-- against the same employee, so the history is continuous and the person is
-- still one row.
--
-- WHAT IS DELIBERATELY NOT HERE
--
-- No salary, no payroll, no leave balances. Compensation is the other half of
-- the effective-dating problem and wants its own record; leave wants an accrual
-- model, and an accrual model written before anybody has asked for a policy is
-- a guess. See ADR 0006 section 9.

-- ---------------------------------------------------------------------------
-- A role, and a place
-- ---------------------------------------------------------------------------
--
-- Both exist independently of anybody holding them: an unfilled vacancy is a
-- job position with nobody assigned to it, which is the only way "what are we
-- recruiting for" can ever be asked.

CREATE TABLE job_positions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Generated (`JOB-###`), on the same terms as a department's code.
    code        TEXT NOT NULL,
    title       TEXT NOT NULL,

    -- The department a role belongs to, where it belongs to one. Optional: a
    -- Health and Safety Officer may sit across the whole organization, and
    -- forcing it into one department would make the org chart lie.
    --
    -- RESTRICT rather than SET NULL: a department with roles defined against it
    -- is not one somebody should be able to delete out from under them.
    department_id UUID REFERENCES departments (id) ON DELETE RESTRICT,

    description TEXT,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT job_positions_code_format CHECK (
        code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$' AND char_length(code) <= 40
    ),
    CONSTRAINT job_positions_title_present CHECK (
        char_length(btrim(title)) BETWEEN 1 AND 120
    ),
    CONSTRAINT job_positions_description_length CHECK (
        description IS NULL OR char_length(description) <= 2000
    )
);

CREATE UNIQUE INDEX job_positions_code_key ON job_positions (lower(code));
CREATE INDEX job_positions_department ON job_positions (department_id);
CREATE INDEX job_positions_active ON job_positions (title) WHERE is_active;

COMMENT ON TABLE job_positions IS
    'A role, which exists whether or not anybody holds it. An unfilled one is a vacancy.';

CREATE TABLE work_locations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    code        TEXT NOT NULL,
    name        TEXT NOT NULL,

    -- office | home | other
    --
    -- Three, and no more. The distinction that earns its place is whether the
    -- workspace controls the premises: it decides who is covered by the
    -- building's insurance and who has to be asked about their own desk.
    kind        TEXT NOT NULL DEFAULT 'office',

    address     TEXT,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT work_locations_code_format CHECK (
        code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$' AND char_length(code) <= 40
    ),
    CONSTRAINT work_locations_name_present CHECK (
        char_length(btrim(name)) BETWEEN 1 AND 120
    ),
    CONSTRAINT work_locations_kind_known CHECK (kind IN ('office', 'home', 'other')),
    CONSTRAINT work_locations_address_length CHECK (
        address IS NULL OR char_length(address) <= 500
    )
);

CREATE UNIQUE INDEX work_locations_code_key ON work_locations (lower(code));
CREATE INDEX work_locations_active ON work_locations (name) WHERE is_active;

-- ---------------------------------------------------------------------------
-- The person
-- ---------------------------------------------------------------------------
--
-- What is here is what stays true when somebody changes job: their name, how to
-- reach them, and who they are. Everything else is dated and lives below.
--
-- There is no `department_id`, no `job_position_id`, no `manager_id` and no
-- `is_active`. Each of those is a fact with a date on it, and putting any of
-- them here is the failure this migration's header describes.

CREATE TABLE employees (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Generated (`EMP-####`). The identifier that survives a rename, a rehire
    -- and a change of legal name, which is why the reports cite it and not the
    -- name.
    code        TEXT NOT NULL,

    -- Two fields, not one. A single `name` cannot be sorted by family name, and
    -- a list of two hundred people that cannot be sorted that way is a list
    -- nobody uses.
    given_name  TEXT NOT NULL,
    family_name TEXT NOT NULL,

    -- What they are actually called, where that is not the given name. Shown in
    -- preference to it everywhere a person is addressed rather than identified.
    preferred_name TEXT,

    work_email  TEXT,
    work_phone  TEXT,

    -- The login, where there is one.
    --
    -- Optional in both directions, and that is the point. Most of the people in
    -- a warehouse never sign in, and a bookkeeper at an outsourced firm signs in
    -- without being an employee. Systems that make an employee *be* a user end
    -- up with a licence bought for somebody who will never use it.
    --
    -- UNIQUE, which is the half that matters: one login is one person. Without
    -- it a mis-keyed link makes two employees the same human being to every
    -- permission check in the system.
    --
    -- SET NULL: deleting a user account must not be blocked by, or delete, the
    -- employment record of the person who held it.
    user_id     UUID REFERENCES core.users (id) ON DELETE SET NULL,

    date_of_birth DATE,

    -- The government's identifier, where the workspace keeps one. Unique where
    -- present: two employees sharing one is either a duplicate person or a
    -- typing error, and both are worth refusing at the point of entry.
    national_id TEXT,

    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT employees_code_format CHECK (
        code ~ '^[A-Za-z0-9][A-Za-z0-9_-]*$' AND char_length(code) <= 40
    ),
    CONSTRAINT employees_given_name_present CHECK (
        char_length(btrim(given_name)) BETWEEN 1 AND 100
    ),
    CONSTRAINT employees_family_name_present CHECK (
        char_length(btrim(family_name)) BETWEEN 1 AND 100
    ),
    CONSTRAINT employees_preferred_name_length CHECK (
        preferred_name IS NULL OR char_length(btrim(preferred_name)) BETWEEN 1 AND 100
    ),
    CONSTRAINT employees_work_email_shape CHECK (
        work_email IS NULL OR (work_email ~ '^[^@[:space:]]+@[^@[:space:]]+$'
                               AND char_length(work_email) <= 320)
    ),
    CONSTRAINT employees_work_phone_length CHECK (
        work_phone IS NULL OR char_length(btrim(work_phone)) BETWEEN 1 AND 40
    ),
    CONSTRAINT employees_national_id_length CHECK (
        national_id IS NULL OR char_length(btrim(national_id)) BETWEEN 1 AND 60
    ),
    CONSTRAINT employees_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    ),
    -- Somebody born after today is a typing error, and it is one that quietly
    -- produces a negative age on every report that shows one.
    CONSTRAINT employees_born_in_the_past CHECK (
        date_of_birth IS NULL OR date_of_birth < CURRENT_DATE
    )
);

CREATE UNIQUE INDEX employees_code_key ON employees (lower(code));
CREATE UNIQUE INDEX employees_user ON employees (user_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX employees_national_id_key
    ON employees (lower(btrim(national_id))) WHERE national_id IS NOT NULL;
CREATE INDEX employees_by_family_name ON employees (lower(family_name), lower(given_name));

COMMENT ON TABLE employees IS
    'A person. Holds only what stays true when they change job - everything dated lives in engagements and assignments.';
COMMENT ON COLUMN employees.user_id IS
    'The login, where there is one. Optional in both directions; unique, because one login is one person.';

-- ---------------------------------------------------------------------------
-- One period of employment
-- ---------------------------------------------------------------------------
--
-- Hired on a date, ended on a date, for a reason. A rehire is a second row
-- against the same employee - see the header.
--
-- "Is this person employed" is therefore a query rather than a flag: an
-- engagement with no end date. That is deliberate. A flag and a set of dates
-- are two facts about the same thing, and the first time somebody backdates a
-- leaving date without clearing the flag they disagree for ever.

CREATE TABLE engagements (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- CASCADE: deleting an employee is only ever allowed for a row created in
    -- error, and an employment period belonging to a person who does not exist
    -- is not something to keep. The service refuses the delete long before this
    -- matters - see `employee::delete`.
    employee_id UUID NOT NULL REFERENCES employees (id) ON DELETE CASCADE,

    started_on  DATE NOT NULL,

    -- NULL means still employed. The only place that fact is recorded.
    ended_on    DATE,

    -- resigned | dismissed | redundancy | end_of_contract | retirement |
    -- died | transferred | other
    --
    -- Required the moment there is an end date, and refused before it. "Fifteen
    -- leavers and no reason on any of them" is one of the named symptoms of an
    -- HR system nobody can report from, and it is the same rule this codebase
    -- applies to a requisition's decision: an outcome carries its reason or it
    -- is not recorded.
    end_reason  TEXT,
    end_note    TEXT,

    -- permanent | fixed_term | contract | intern | casual | apprentice
    --
    -- On the engagement rather than the assignment: it is a property of the
    -- employment, and somebody who moves from intern to permanent has started a
    -- different employment rather than changed desks.
    employment_type TEXT NOT NULL DEFAULT 'permanent',

    -- Where the type is fixed term, when it was agreed to run to. Distinct from
    -- `ended_on`, which is when it actually did: a contract extended twice and a
    -- contract ended early are both worth being able to see.
    expected_end_on DATE,

    note        TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT engagements_end_reason_known CHECK (
        end_reason IS NULL OR end_reason IN (
            'resigned', 'dismissed', 'redundancy', 'end_of_contract',
            'retirement', 'died', 'transferred', 'other'
        )
    ),
    CONSTRAINT engagements_type_known CHECK (
        employment_type IN (
            'permanent', 'fixed_term', 'contract', 'intern', 'casual', 'apprentice'
        )
    ),
    -- An end date and a reason are one fact. Either both or neither.
    CONSTRAINT engagements_ended_with_a_reason CHECK (
        (ended_on IS NOT NULL) = (end_reason IS NOT NULL)
    ),
    CONSTRAINT engagements_ended_after_starting CHECK (
        ended_on IS NULL OR ended_on >= started_on
    ),
    CONSTRAINT engagements_expected_after_starting CHECK (
        expected_end_on IS NULL OR expected_end_on >= started_on
    ),
    CONSTRAINT engagements_end_note_length CHECK (
        end_note IS NULL OR char_length(end_note) <= 2000
    ),
    CONSTRAINT engagements_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    )
);

-- One open engagement per person. Somebody employed twice at once is either two
-- people or a leaving date somebody forgot, and both are worth refusing.
--
-- This does not stop two CLOSED engagements overlapping. That would want an
-- exclusion constraint over a date range, which is cross-row machinery of the
-- kind this schema keeps out of the database on purpose - see ADR 0006. The
-- service walks the chain instead.
CREATE UNIQUE INDEX engagements_one_open_per_employee
    ON engagements (employee_id) WHERE ended_on IS NULL;

CREATE INDEX engagements_employee ON engagements (employee_id, started_on DESC);
CREATE INDEX engagements_current ON engagements (started_on DESC) WHERE ended_on IS NULL;

COMMENT ON TABLE engagements IS
    'One period of employment. A rehire is a second row against the same employee, so tenure history survives.';
COMMENT ON COLUMN engagements.ended_on IS
    'NULL means still employed. The only place that fact is recorded - there is deliberately no is_active flag to disagree with it.';

-- ---------------------------------------------------------------------------
-- What somebody was doing, and when
-- ---------------------------------------------------------------------------
--
-- The table this migration exists for. Every one of these columns is a column
-- another system would have put on the employee row, and every one of them
-- changes.

CREATE TABLE assignments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    engagement_id UUID NOT NULL REFERENCES engagements (id) ON DELETE CASCADE,

    effective_from DATE NOT NULL,

    -- NULL means current. Closed by the service when the next one opens, which
    -- is what makes the chain a chain.
    effective_to   DATE,

    -- RESTRICT throughout: a department, role or location that people are
    -- assigned to is not one somebody should be able to delete. Deactivation is
    -- the way to retire one, exactly as it is for a department.
    department_id     UUID REFERENCES departments (id) ON DELETE RESTRICT,
    job_position_id   UUID REFERENCES job_positions (id) ON DELETE RESTRICT,
    work_location_id  UUID REFERENCES work_locations (id) ON DELETE RESTRICT,

    -- Who they report to, as an EMPLOYEE rather than a user: most managers do
    -- not have a login either, and a reporting line that only exists for people
    -- with accounts is an org chart with holes in it.
    --
    -- SET NULL: a manager who leaves must not block the deletion of a record
    -- created in error, and must not silently take their reports with them. The
    -- gap is visible, which is the correct outcome - somebody has to be told to
    -- reassign them.
    manager_id UUID REFERENCES employees (id) ON DELETE SET NULL,

    -- Why this assignment started. Free text rather than a list: "moved to the
    -- new team" and "covering maternity leave" are both true and neither is an
    -- enum anybody would have guessed.
    reason      TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT assignments_ends_after_starting CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    CONSTRAINT assignments_reason_length CHECK (
        reason IS NULL OR char_length(reason) <= 500
    ),
    -- Row-local, so it belongs here. The deeper case - A reports to B who
    -- reports to A - cannot be seen from one row and is the service's.
    CONSTRAINT assignments_not_own_manager CHECK (manager_id IS DISTINCT FROM id)
);

-- One current assignment per engagement. Two open rows would be a person in two
-- departments at once, and every cost report would count them twice.
CREATE UNIQUE INDEX assignments_one_open_per_engagement
    ON assignments (engagement_id) WHERE effective_to IS NULL;

CREATE INDEX assignments_engagement ON assignments (engagement_id, effective_from DESC);
CREATE INDEX assignments_department ON assignments (department_id);
CREATE INDEX assignments_manager ON assignments (manager_id);
CREATE INDEX assignments_job ON assignments (job_position_id);

COMMENT ON TABLE assignments IS
    'What somebody was doing between two dates: department, role, manager, place. Dated rather than stored on the employee, so "which department were they in last March" has an answer.';

-- ---------------------------------------------------------------------------
-- Who is here now
-- ---------------------------------------------------------------------------
--
-- The join every screen wants, written once. Three tables and two open-row
-- rules is the price of keeping the history; a view is what stops that price
-- being paid again in every query.
--
-- `current_headcount` below is what a cost report groups by, and it is the
-- reason `assignments.department_id` is nullable: somebody hired before anybody
-- decided where they sit still has to appear.

CREATE VIEW current_staff AS
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
       m.family_name              AS manager_family_name
  FROM employees e
  JOIN engagements g ON g.employee_id = e.id AND g.ended_on IS NULL
  LEFT JOIN assignments a ON a.engagement_id = g.id AND a.effective_to IS NULL
  LEFT JOIN departments d ON d.id = a.department_id
  LEFT JOIN job_positions j ON j.id = a.job_position_id
  LEFT JOIN work_locations w ON w.id = a.work_location_id
  LEFT JOIN employees m ON m.id = a.manager_id;

COMMENT ON VIEW current_staff IS
    'Everybody currently employed, with what they are currently doing. An employee with no open engagement is absent from it, which is what "no longer here" means.';

-- What each department costs in people, which is the question the cost centre
-- was built for and the one a flat employee table answers wrongly the first
-- time anybody moves.
CREATE VIEW current_headcount AS
SELECT d.id            AS department_id,
       d.code,
       d.name,
       d.is_cost_centre,
       count(s.employee_id) AS headcount
  FROM departments d
  LEFT JOIN current_staff s ON s.department_id = d.id
 GROUP BY d.id, d.code, d.name, d.is_cost_centre;
