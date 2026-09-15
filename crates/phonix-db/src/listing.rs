//! The piece of a paged listing that must not be written twice.
//!
//! A `PageRequest` arrives from a browser, and `sort.field` is a string in it.
//! The only safe way to put that string in an `ORDER BY` is to not put it
//! there: to match it against a list of literals the reader wrote itself and
//! use the literal. Every paged reader in this crate needs that lookup, and a
//! lookup written nine times is eight chances to write it as interpolation
//! instead.
//!
//! The other half of a listing - the `WHERE`, the count, the binds - stays in
//! the module that owns the table. It is different every time, and a builder
//! general enough to express all of it would be harder to read than the SQL.

use phonix_core::query::Sort;

/// A column a grid may order by: the name the browser knows, and the SQL.
///
/// The first is a grid's `field`; the second is a column or an expression this
/// crate wrote. Nothing from a request ever becomes the second.
pub type Sortable = (&'static str, &'static str);

/// The `ORDER BY` fragment a request asks for.
///
/// `fallback` is used when nothing was asked, and when what was asked for is
/// not in `sortable` - a sort naming a column this build does not know comes
/// from a browser running a newer screen, and the answer to that is the list in
/// its usual order rather than an error page.
///
/// The direction is safe to interpolate because it is one of two literals; see
/// [`SortDirection::sql`](phonix_core::query::SortDirection::sql).
pub fn order_by(sort: Option<&Sort>, sortable: &[Sortable], fallback: &str) -> String {
    sort.and_then(|sort| {
        sortable
            .iter()
            .find(|(field, _)| *field == sort.field)
            .map(|(_, column)| format!("{column} {}", sort.direction.sql()))
    })
    .unwrap_or_else(|| fallback.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SORTABLE: &[Sortable] = &[("number", "d.number"), ("issued_on", "d.issued_on")];

    #[test]
    fn a_known_column_is_ordered_by_the_sql_this_crate_wrote() {
        let sort = Sort::descending("number");

        assert_eq!(
            order_by(Some(&sort), SORTABLE, "d.created_at DESC"),
            "d.number DESC"
        );
    }

    #[test]
    fn nothing_asked_is_the_list_in_its_usual_order() {
        assert_eq!(
            order_by(None, SORTABLE, "d.created_at DESC"),
            "d.created_at DESC"
        );
    }

    #[test]
    fn a_column_this_build_does_not_know_is_ignored_rather_than_refused() {
        // From a browser running a newer screen. The answer is the list, not an
        // error page - and never the string itself.
        let sort = Sort::ascending("invented_by_a_newer_screen");

        assert_eq!(
            order_by(Some(&sort), SORTABLE, "d.created_at DESC"),
            "d.created_at DESC"
        );
    }

    #[test]
    fn nothing_from_the_request_reaches_the_fragment() {
        let sort = Sort::ascending("number; DROP TABLE books.invoices --");

        let clause = order_by(Some(&sort), SORTABLE, "d.created_at DESC");

        assert!(!clause.contains("DROP"));
    }
}
