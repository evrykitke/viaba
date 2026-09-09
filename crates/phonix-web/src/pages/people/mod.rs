//! People: how this workspace is arranged, and what it charges to.
//!
//! Not under `/admin`: a requisition names a cost centre before it names
//! anything else, so most of a workspace reads this list.
//!
//! ```text
//! /people                     the app's home
//! /people/departments         the list, drawn as a tree
//! /people/departments/new     a form
//! /people/departments/:id     Details | History
//! /people/employees           who works here, leavers included
//! /people/employees/new       the person, and the job they are starting
//! /people/employees/:id       the record: employment, assignments, login
//! /people/roles               the roles the organization is made of
//! /people/roles/:id           a form
//! /people/places              where people work
//! /people/places/:id          a form
//! ```
//!
//! There is no separate cost-centre screen: a cost centre is a department with
//! a flag, so it is a filter on the grid rather than a second screen.

pub mod department;
pub mod employee;
pub mod departments;
pub mod home;
pub mod lists;
pub mod reference;
