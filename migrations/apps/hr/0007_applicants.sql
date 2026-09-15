-- hr 0007: who has applied, against the vacancies that already exist.
--
-- `job_positions` are rows precisely so that a vacancy is a thing rather than
-- text typed onto somebody, and until now nothing answered "who wants it".
--
-- AN APPLICANT IS NOT AN EMPLOYEE
--
-- Most applicants never become one, so they are not `employees` rows with a
-- flag. The reverse matters more: an applicant who IS hired must become an
-- engagement on whichever employee record they already have, not a second
-- person. That is the duplicate-identity case 0002 was written to refuse -
-- somebody who left in 2019 and applies again in 2026 is one human being with
-- two periods of employment, not two people sharing a national insurance
-- number and a tenure figure that starts again from zero.
--
-- So `employee_id` is set at hire and is nullable until then. A hire fills it
-- with an existing record where one is found, and with a newly created one
-- where none is.
--
-- STAGES ARE A CLOSED LIST
--
-- Frappe HR's Job Applicant carries a status and this follows it, with the
-- stages named for what is happening rather than for how it feels: applied,
-- screening, interview, offer, hired, rejected, withdrawn. A workspace wanting
-- its own pipeline wants a table of stages and a per-stage order, which is a
-- bigger thing than this, and building the closed list first is what makes the
-- bigger thing answerable later.

CREATE TABLE applicants (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Which vacancy. RESTRICT: a role people have applied for is not one to
    -- delete, exactly as one people are assigned to is not.
    job_position_id UUID NOT NULL REFERENCES job_positions (id) ON DELETE RESTRICT,

    -- Their own name, not an employee's: this person may never be one, and the
    -- name they applied under is what the correspondence says.
    given_name      TEXT NOT NULL,
    family_name     TEXT NOT NULL,

    email           TEXT,
    phone           TEXT,

    -- applied | screening | interview | offer | hired | rejected | withdrawn
    stage           TEXT NOT NULL DEFAULT 'applied',

    -- Where they came from: a job board, a referral, a walk-in. Free text
    -- rather than a list, because the answer is "Rachel knows him" as often as
    -- it is the name of a website.
    source          TEXT,

    applied_on      DATE NOT NULL DEFAULT current_date,

    note            TEXT,

    -- Set when they are hired, and only then. The whole point: a hire points at
    -- the person they became rather than creating a second one.
    --
    -- SET NULL rather than CASCADE: deleting an employee record created in
    -- error must not silently take the application with it, because the
    -- application is evidence that somebody applied.
    employee_id     UUID REFERENCES employees (id) ON DELETE SET NULL,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT applicants_stage_known CHECK (
        stage IN ('applied', 'screening', 'interview', 'offer',
                  'hired', 'rejected', 'withdrawn')
    ),
    CONSTRAINT applicants_given_name_present CHECK (
        char_length(btrim(given_name)) BETWEEN 1 AND 100
    ),
    CONSTRAINT applicants_family_name_present CHECK (
        char_length(btrim(family_name)) BETWEEN 1 AND 100
    ),
    CONSTRAINT applicants_email_shape CHECK (
        email IS NULL OR (email LIKE '%_@_%' AND char_length(email) <= 320)
    ),
    CONSTRAINT applicants_phone_length CHECK (
        phone IS NULL OR char_length(phone) <= 40
    ),
    CONSTRAINT applicants_source_length CHECK (
        source IS NULL OR char_length(source) <= 200
    ),
    CONSTRAINT applicants_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    ),

    -- Hired means there is somebody they became. Row-local, so it belongs here.
    CONSTRAINT applicants_hired_has_a_person CHECK (
        stage <> 'hired' OR employee_id IS NOT NULL
    )
);

-- What the vacancy screen reads: who has applied for this role.
CREATE INDEX applicants_position ON applicants (job_position_id, applied_on DESC);

-- What the pipeline reads: everybody still in play, newest first.
CREATE INDEX applicants_open ON applicants (applied_on DESC)
    WHERE stage NOT IN ('hired', 'rejected', 'withdrawn');

-- Answering "has this person applied before" without a full scan.
CREATE INDEX applicants_email ON applicants (lower(email))
    WHERE email IS NOT NULL;

COMMENT ON TABLE applicants IS
    'Who has applied for a job position. Hiring one points `employee_id` at the person they became - an existing record where there is one, so a rehire is a second engagement rather than a second person.';
