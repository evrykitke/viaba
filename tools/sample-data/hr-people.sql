-- Sample data for med-app-staging: people, and the fact that what they do
-- changes.
--
-- HOW TO RUN IT
--
--   psql "<med-app-staging connection string>" -f tools/sample-data/hr-people.sql
--
-- Against the TENANT database, not the catalog. Every statement is idempotent:
-- run it twice and nothing doubles.
--
-- It expects the three departments `inventory-procurement.sql` seeds - Clinic
-- A, Clinic B and Theatre - and creates them itself if that script has not been
-- run, so the two can be run in either order.
--
-- WHAT IT SEEDS, AND WHY EACH PERSON IS THERE
--
-- Nine people, arranged so every case this app was built for is one click away
-- rather than something you have to construct:
--
--   Margaret Okonkwo   Matron, Clinic A. MOVED once - started as a Staff Nurse
--                      in Clinic B and was promoted in April. Her assignment
--                      history is the whole point of the schema: the current
--                      answer is Clinic A, and last February's answer is still
--                      Clinic B.
--
--   Daniel Achebe      Staff Nurse, Clinic A. Reports to Margaret. Has a work
--                      email, so he is the one to press "Create a login" on.
--
--   Grace Mwangi       Staff Nurse, Clinic B. No work email at all, which is
--                      the ordinary case for most of a workforce - the login
--                      panel says why it cannot be used rather than hiding.
--
--   Tunde Balogun      Porter, Theatre. No email either.
--
--   Amara Nwosu        Buyer, Clinic A. FIXED TERM, and its agreed end date has
--                      already passed - so her record carries the overrunning
--                      warning. A contract extended verbally and written up
--                      late is the ordinary case, which is why it warns rather
--                      than refuses.
--
--   Peter Adeyemi      LEFT. Resigned in March. He is still on the staff list,
--                      because a list that dropped somebody the day they left
--                      is one nobody can look a former colleague up in.
--
--   Ruth Kimani        REHIRED. Worked here 2023-2024, left, and came back in
--                      January - as TWO engagements against ONE person, which
--                      is the case that makes a flat employee table produce two
--                      people and a tenure figure that restarts at zero.
--
--   Samuel Eze         Intern, Theatre. Started last month.
--
--   Blessing Obi       Cleaner, Clinic B. Works at a client site rather than
--                      the office, so the places list is not all one kind.
--
-- WHAT IT DELIBERATELY DOES NOT SEED
--
-- A login for anybody. `employees.user_id` stays NULL on all nine, because a
-- login is created through the invitation flow - the person sets their own
-- password and the address is proved by the link being opened. A seed script
-- writing a `core.users` row and pointing an employee at it would be
-- fabricating exactly the thing that flow exists to establish.
--
-- Press "Create a login" on Daniel Achebe to see it work.

BEGIN;

SET LOCAL search_path = hr, public;

-- ---------------------------------------------------------------------------
-- What this script needs to already be there
-- ---------------------------------------------------------------------------

DO $$
BEGIN
    IF to_regclass('hr.employees') IS NULL THEN
        RAISE EXCEPTION 'hr.employees is missing. Run the app once so migration 0002 applies.';
    END IF;
END $$;

-- The three departments, in case the inventory sample data has not been run.
INSERT INTO departments (code, name, is_cost_centre, is_active)
SELECT v.code, v.name, TRUE, TRUE
  FROM (VALUES
        ('CLINIC-A', 'Clinic A'),
        ('CLINIC-B', 'Clinic B'),
        ('THEATRE', 'Theatre')
       ) AS v(code, name)
 WHERE NOT EXISTS (
     SELECT 1 FROM departments d WHERE lower(d.code) = lower(v.code)
 );

-- ---------------------------------------------------------------------------
-- Where people work
-- ---------------------------------------------------------------------------
--
-- Three kinds, because the distinction that earns its place is whether the
-- workspace controls the premises: it decides who is on the fire register.

INSERT INTO work_locations (code, name, kind, address, is_active)
SELECT v.code, v.name, v.kind, v.address, TRUE
  FROM (VALUES
        ('LOC-HQ', 'Main clinic', 'office',
         '14 Ridgeway, Nairobi'),
        ('LOC-ANNEX', 'Theatre annex', 'office',
         '14 Ridgeway, Nairobi (rear building)'),
        ('LOC-HOME', 'Home', 'home', NULL),
        ('LOC-FIELD', 'Client sites', 'other', NULL)
       ) AS v(code, name, kind, address)
 WHERE NOT EXISTS (
     SELECT 1 FROM work_locations w WHERE lower(w.code) = lower(v.code)
 );

-- ---------------------------------------------------------------------------
-- The roles
-- ---------------------------------------------------------------------------
--
-- Six defined and five filled: `JOB-PHARM` is a vacancy, which is the only way
-- "what are we recruiting for" can be asked at all. A role is a row precisely
-- so the job nobody is doing still has one.

INSERT INTO job_positions (code, title, department_id, description, is_active)
SELECT v.code, v.title, d.id, v.description, TRUE
  FROM (VALUES
        ('JOB-MATRON', 'Matron', 'CLINIC-A',
         'Runs the clinic floor and the nursing rota.'),
        ('JOB-NURSE', 'Staff Nurse', NULL,
         'Ward nursing. Deliberately belongs to no one department - nurses move between them.'),
        ('JOB-PORTER', 'Porter', 'THEATRE', NULL),
        ('JOB-BUYER', 'Buyer', 'CLINIC-A',
         'Raises consolidations and purchase orders.'),
        ('JOB-CLEANER', 'Cleaner', 'CLINIC-B', NULL),
        ('JOB-PHARM', 'Pharmacist', 'CLINIC-A',
         'Nobody holds this one. It is the vacancy.')
       ) AS v(code, title, department, description)
  LEFT JOIN departments d ON lower(d.code) = lower(v.department)
 WHERE NOT EXISTS (
     SELECT 1 FROM job_positions j WHERE lower(j.code) = lower(v.code)
 );

-- ---------------------------------------------------------------------------
-- The people
-- ---------------------------------------------------------------------------
--
-- Codes are keyed rather than drawn from the series - a seed script must not
-- spend numbers a real record will want - so they sit in a SAMPLE- range no
-- series will ever produce.
--
-- `user_id` is NULL on every one of them. See the header.

INSERT INTO employees (code, given_name, family_name, preferred_name,
                       work_email, work_phone, date_of_birth, note)
SELECT v.code, v.given_name, v.family_name, v.preferred_name,
       v.work_email, v.work_phone, v.born, v.note
  FROM (VALUES
        ('SAMPLE-EMP-01', 'Margaret', 'Okonkwo', 'Maggie',
         'margaret.okonkwo@example.test', '+254 700 000 001', DATE '1981-04-12',
         'Sample data. Promoted in April - her assignment history is the point of the schema.'),
        ('SAMPLE-EMP-02', 'Daniel', 'Achebe', NULL,
         'daniel.achebe@example.test', '+254 700 000 002', DATE '1994-09-30',
         'Sample data. Has a work email, so a login can be created for him.'),
        ('SAMPLE-EMP-03', 'Grace', 'Mwangi', NULL,
         NULL, '+254 700 000 003', DATE '1990-01-22',
         'Sample data. No work email - the ordinary case, and the login panel says so.'),
        ('SAMPLE-EMP-04', 'Tunde', 'Balogun', NULL,
         NULL, NULL, DATE '1988-07-05', 'Sample data.'),
        ('SAMPLE-EMP-05', 'Amara', 'Nwosu', NULL,
         'amara.nwosu@example.test', NULL, DATE '1992-11-17',
         'Sample data. Fixed term, already past the date it was agreed to run to.'),
        ('SAMPLE-EMP-06', 'Peter', 'Adeyemi', NULL,
         'peter.adeyemi@example.test', NULL, DATE '1985-02-28',
         'Sample data. Left in March, and still on the list.'),
        ('SAMPLE-EMP-07', 'Ruth', 'Kimani', NULL,
         'ruth.kimani@example.test', NULL, DATE '1996-06-11',
         'Sample data. Worked here twice - two engagements, one person.'),
        ('SAMPLE-EMP-08', 'Samuel', 'Eze', 'Sam',
         'samuel.eze@example.test', NULL, DATE '2003-03-08', 'Sample data.'),
        ('SAMPLE-EMP-09', 'Blessing', 'Obi', NULL,
         NULL, NULL, DATE '1979-12-01', 'Sample data.')
       ) AS v(code, given_name, family_name, preferred_name, work_email,
              work_phone, born, note)
 WHERE NOT EXISTS (
     SELECT 1 FROM employees e WHERE e.code = v.code
 );

-- ---------------------------------------------------------------------------
-- Their employment
-- ---------------------------------------------------------------------------
--
-- One row per period. Ruth gets two, which is what a rehire is: the first
-- engagement keeps its dates and its reason, and the second stands beside it.
-- A system that modelled employment as a flag on the person would have had to
-- choose between losing the first stint and inventing a second Ruth.

INSERT INTO engagements (employee_id, started_on, ended_on, end_reason, end_note,
                         employment_type, expected_end_on)
SELECT e.id, v.started_on, v.ended_on, v.end_reason, v.end_note,
       v.employment_type, v.expected_end_on
  FROM (VALUES
        ('SAMPLE-EMP-01', DATE '2019-02-01', NULL, NULL, NULL, 'permanent', NULL),
        ('SAMPLE-EMP-02', DATE '2022-06-15', NULL, NULL, NULL, 'permanent', NULL),
        ('SAMPLE-EMP-03', DATE '2021-09-01', NULL, NULL, NULL, 'permanent', NULL),
        ('SAMPLE-EMP-04', DATE '2020-03-10', NULL, NULL, NULL, 'permanent', NULL),
        -- Fixed term, agreed to run to a date that has already gone by.
        ('SAMPLE-EMP-05', DATE '2025-10-01', NULL, NULL, NULL, 'fixed_term',
         DATE '2026-06-30'),
        ('SAMPLE-EMP-06', DATE '2018-01-08', DATE '2026-03-31', 'resigned',
         'Moved to another hospital. Sample data.', 'permanent', NULL),
        -- Ruth, first time round.
        ('SAMPLE-EMP-07', DATE '2023-02-01', DATE '2024-08-31', 'resigned',
         'Went back to study. Sample data.', 'permanent', NULL),
        -- Ruth, second time round. Same person, second engagement.
        ('SAMPLE-EMP-07', DATE '2026-01-12', NULL, NULL, NULL, 'permanent', NULL),
        ('SAMPLE-EMP-08', DATE '2026-08-03', NULL, NULL, NULL, 'intern',
         DATE '2027-02-03'),
        ('SAMPLE-EMP-09', DATE '2017-11-20', NULL, NULL, NULL, 'casual', NULL)
       ) AS v(code, started_on, ended_on, end_reason, end_note, employment_type,
              expected_end_on)
  JOIN employees e ON e.code = v.code
 WHERE NOT EXISTS (
     SELECT 1 FROM engagements g
      WHERE g.employee_id = e.id AND g.started_on = v.started_on
 );

-- ---------------------------------------------------------------------------
-- What they were doing, and when
-- ---------------------------------------------------------------------------
--
-- The table this app exists for. Margaret has TWO rows against one engagement:
-- Staff Nurse in Clinic B until April, Matron in Clinic A since. A system that
-- kept her department as a column on the person would show Clinic A and have
-- nothing at all to say about February.
--
-- The manager is resolved by employee code rather than by id, so the chain is
-- readable here and does not depend on insertion order.

INSERT INTO assignments (engagement_id, effective_from, effective_to,
                         department_id, job_position_id, work_location_id,
                         manager_id, reason)
SELECT g.id, v.effective_from, v.effective_to, d.id, j.id, w.id, m.id, v.reason
  FROM (VALUES
        -- Margaret: the move that makes the history worth keeping.
        ('SAMPLE-EMP-01', DATE '2019-02-01', DATE '2026-03-31', DATE '2019-02-01',
         'CLINIC-B', 'JOB-NURSE', 'LOC-HQ', NULL, NULL),
        ('SAMPLE-EMP-01', DATE '2019-02-01', NULL, DATE '2026-04-01',
         'CLINIC-A', 'JOB-MATRON', 'LOC-HQ', NULL, 'Promoted to Matron.'),

        ('SAMPLE-EMP-02', DATE '2022-06-15', NULL, DATE '2022-06-15',
         'CLINIC-A', 'JOB-NURSE', 'LOC-HQ', 'SAMPLE-EMP-01', NULL),
        ('SAMPLE-EMP-03', DATE '2021-09-01', NULL, DATE '2021-09-01',
         'CLINIC-B', 'JOB-NURSE', 'LOC-HQ', 'SAMPLE-EMP-01', NULL),
        ('SAMPLE-EMP-04', DATE '2020-03-10', NULL, DATE '2020-03-10',
         'THEATRE', 'JOB-PORTER', 'LOC-ANNEX', NULL, NULL),
        ('SAMPLE-EMP-05', DATE '2025-10-01', NULL, DATE '2025-10-01',
         'CLINIC-A', 'JOB-BUYER', 'LOC-HOME', NULL, NULL),
        -- Peter's assignment closed with his engagement, on the day he left.
        ('SAMPLE-EMP-06', DATE '2018-01-08', DATE '2026-03-31', DATE '2018-01-08',
         'THEATRE', 'JOB-PORTER', 'LOC-ANNEX', NULL, NULL),
        -- Ruth's first stint, closed; and her second, open.
        ('SAMPLE-EMP-07', DATE '2023-02-01', DATE '2024-08-31', DATE '2023-02-01',
         'CLINIC-B', 'JOB-NURSE', 'LOC-HQ', NULL, NULL),
        ('SAMPLE-EMP-07', DATE '2026-01-12', NULL, DATE '2026-01-12',
         'CLINIC-A', 'JOB-NURSE', 'LOC-HQ', 'SAMPLE-EMP-01', 'Rehired.'),
        ('SAMPLE-EMP-08', DATE '2026-08-03', NULL, DATE '2026-08-03',
         'THEATRE', 'JOB-PORTER', 'LOC-ANNEX', 'SAMPLE-EMP-04', NULL),
        ('SAMPLE-EMP-09', DATE '2017-11-20', NULL, DATE '2017-11-20',
         'CLINIC-B', 'JOB-CLEANER', 'LOC-FIELD', NULL, NULL)
       ) AS v(code, engagement_started, effective_to, effective_from,
              department, job, place, manager_code, reason)
  JOIN employees e ON e.code = v.code
  JOIN engagements g ON g.employee_id = e.id AND g.started_on = v.engagement_started
  LEFT JOIN departments d ON lower(d.code) = lower(v.department)
  LEFT JOIN job_positions j ON lower(j.code) = lower(v.job)
  LEFT JOIN work_locations w ON lower(w.code) = lower(v.place)
  LEFT JOIN employees m ON m.code = v.manager_code
 WHERE NOT EXISTS (
     SELECT 1 FROM assignments a
      WHERE a.engagement_id = g.id AND a.effective_from = v.effective_from
 );

COMMIT;

-- ---------------------------------------------------------------------------
-- The walk-through
-- ---------------------------------------------------------------------------
--
--   1. People > People. Nine rows. Eight read Employed and Peter Adeyemi reads
--      Left - and he is still on the list, which is the point: a staff list
--      that hides leavers is one nobody can look a former colleague up in.
--      Filter by state to see the two apart.
--
--      Every row's Login column is blank. Nobody is seeded with an account,
--      because most people who work somewhere never sign in.
--
--   2. Open Margaret Okonkwo. Her employment panel shows one period and TWO
--      assignments:
--
--        2019-02-01 → 2026-03-31   Clinic B    Staff Nurse
--        2026-04-01 → current      Clinic A    Matron      "Promoted to Matron."
--
--      That is the whole reason this app is five tables. Ask any other HR
--      system what department she was in last February and it will say Clinic
--      A, because it stored one column and overwrote it.
--
--   3. Press "Move them" on her. The form opens already holding what she is
--      doing now, so a move only has to change what moved. Change the place to
--      Theatre annex, give a reason, and record it: a third row appears and the
--      second is closed the day before the third begins - never both covering
--      the changeover date, which is how a headcount report counts somebody
--      twice.
--
--   4. Open Ruth Kimani. TWO periods of employment against one person:
--      2023-2024, resigned, and 2026 to now. Her first stint keeps its dates
--      and its reason. This is the rehire case that makes a flat employee table
--      produce either two Ruths or one with her history erased.
--
--   5. Open Amara Nwosu. Fixed term, and the record warns that it has run past
--      the date it was agreed to end on. A warning rather than a refusal:
--      contracts get extended verbally and written up late, and refusing to
--      show her would not make the paperwork appear.
--
--   6. Open Daniel Achebe and press "Create a login".
--
--      It sends an invitation to his work email and he sets his own password -
--      nobody, including whoever pressed the button, ever knows it. The row's
--      Login column then shows a key.
--
--      If the workspace has no mail relay configured the account is still
--      created and the screen hands you the link to send another way. That is
--      deliberate: an account that exists with an undelivered invitation is
--      recoverable, and a request that rolled back because a mail server was
--      briefly unreachable is not.
--
--   7. Now open Grace Mwangi and look at the same panel. There is no button,
--      and the panel says why: she has no work email for an invitation to go
--      to. Not everybody who works here signs in, and the screen says so rather
--      than offering something that would fail.
--
--   8. People > Roles. Six rows, and Pharmacist reads Vacant. A role exists
--      whether or not anybody holds it, which is the only way an unfilled
--      position can be seen at all - a job title typed onto a person has no row
--      for the job nobody is doing. `filled` is counted over the open
--      assignments, so it goes stale the moment somebody leaves rather than
--      when somebody remembers.
--
--   9. People > Places. Four rows across three kinds. The kind is not
--      decoration: it says whether the workspace controls the premises, which
--      decides who is on the fire register.
--
--  10. Record a leaver. Open Samuel Eze, press "Record a leaver", and try to
--      save without changing the reason - it takes the default, because an end
--      date and a reason are one fact and the schema refuses either alone.
--      Then look at him on the list: Left, and still there.
--
-- WHAT SHOULD BE TRUE AFTERWARDS
--
--   -- Eight employed, and Peter absent from it. `current_staff` is the join
--   -- every screen wants, written once.
--   SELECT code, given_name, family_name, department_name, job_title
--     FROM hr.current_staff ORDER BY family_name;
--
--   -- What each department costs in people, which is the question the cost
--   -- centre was built for.
--   SELECT name, headcount FROM hr.current_headcount ORDER BY name;
--
--   -- Margaret in February, which is the query no other system can answer.
--   SELECT d.name, a.effective_from, a.effective_to
--     FROM hr.assignments a
--     JOIN hr.engagements g ON g.id = a.engagement_id
--     JOIN hr.employees e ON e.id = g.employee_id
--     LEFT JOIN hr.departments d ON d.id = a.department_id
--    WHERE e.code = 'SAMPLE-EMP-01'
--      AND DATE '2026-02-15' BETWEEN a.effective_from
--                            AND COALESCE(a.effective_to, 'infinity'::date);
--
--   -- Ruth's whole service, across both stints.
--   SELECT started_on, ended_on, end_reason FROM hr.engagements g
--     JOIN hr.employees e ON e.id = g.employee_id
--    WHERE e.code = 'SAMPLE-EMP-07' ORDER BY started_on;
--
--   -- Nobody has a login until you make one.
--   SELECT count(*) FROM hr.employees WHERE user_id IS NOT NULL;
