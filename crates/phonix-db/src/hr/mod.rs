//! The `hr` app's tables. One so far: departments.
//!
//! Statements are qualified — `hr.departments`, never `departments` — because a
//! request runs on `core,public` and an unqualified reference should fail
//! loudly rather than resolve by luck.
//!
//! Nothing outside `hr` holds a foreign key into it, which is what keeps
//! `DROP SCHEMA hr CASCADE` safe. The cost is that this schema cannot ask
//! whether a department has been charged to; see [`department::delete`].

pub mod department;
