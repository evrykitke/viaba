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
//! ```
//!
//! There is no separate cost-centre screen: a cost centre is a department with
//! a flag, so it is a filter on the grid rather than a second screen.

pub mod department;
pub mod departments;
pub mod home;
