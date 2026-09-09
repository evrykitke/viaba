//! The `hr` app's tables: who works here, how the workspace is arranged, and
//! what it charges to.
//!
//! Statements are qualified — `hr.departments`, never `departments` — because a
//! request runs on `core,public` and an unqualified reference should fail
//! loudly rather than resolve by luck.
//!
//! Nothing outside `hr` holds a foreign key into it, which is what keeps
//! `DROP SCHEMA hr CASCADE` safe. The cost is that this schema cannot ask
//! whether a department has been charged to; see [`department::delete`].
//!
//! # The one foreign key that points outwards
//!
//! `employees.user_id` references `core.users`, which is allowed because `core`
//! is not an app — every schema may point at it. It is `ON DELETE SET NULL`, so
//! closing an account never takes an employment record with it.
//!
//! # What is dated and what is not
//!
//! [`employee`] is three tables and a view rather than one wide row, and the
//! reason is in the head of `migrations/apps/hr/0002_people.sql`: a system that
//! keeps somebody's current department as a column can never say what it was
//! last March.

pub mod department;
pub mod employee;
pub mod job_position;
pub mod work_location;

use crate::error::DbError;

/// Turn a unique-index violation on a code into the error a form can show.
///
/// One function for four tables. Each passes its own index name and entity,
/// because the message names the entity and the caller is the only thing that
/// knows which index it was about to violate.
pub(crate) fn code_conflict(
    err: sqlx::Error,
    entity: &'static str,
    index: &str,
    code: &str,
) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(index) => DbError::CodeExists {
            entity,
            code: code.to_owned(),
        },
        _ => DbError::Query(err),
    }
}
