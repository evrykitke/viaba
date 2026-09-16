//! What a column is: an identifier, a heading, and one way to read a row.
//!
//! # One extractor, four jobs
//!
//! A column declares a single function from a row to a [`Cell`]. That one
//! function is what the grid searches, what it sorts by, what it exports, and -
//! unless the column overrides it - what it draws.
//!
//! The alternative is the usual one: a `render` for the screen, a `sort_key`
//! for ordering, a `search_text` for filtering, an `export` for CSV. Four
//! chances to describe the same column four slightly different ways, and the
//! bug that follows is always the same shape - a column that shows a formatted
//! date, sorts as a string, and puts "3 days ago" in the export.
//!
//! So [`Column::render`] is deliberately narrow: it changes how a value
//! *looks*, never what it *is*. A status column may draw a coloured badge, and
//! it still sorts and exports as the word inside the badge.
//!
//! # Declaring one
//!
//! ```ignore
//! Column::new("last_login_at", "Last sign-in", |u: &UserListing| {
//!     u.last_login_at.map_or(Cell::Empty, Cell::timestamp)
//! })
//! .sortable()
//! .align(Align::End)
//! ```
//!
//! `field` is a stable identifier, not a heading: it keys the sort, the column
//! toggle and - for a server-side source - the `ORDER BY`. Renaming the heading
//! is a wording change; renaming the field is a contract change.

use std::sync::Arc;

use leptos::prelude::*;

pub use phonix_core::report::Cell;

/// Which edge of its cell a column's content sits against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

impl Align {
    pub const fn cell_class(self) -> &'static str {
        match self {
            Self::Start => "text-left",
            Self::Center => "text-center",
            Self::End => "text-right",
        }
    }
}

/// How a row is read for one column.
type Read<T> = Arc<dyn Fn(&T) -> Cell + Send + Sync>;

/// How a cell is drawn, when plain text will not do.
type Draw<T> = Arc<dyn Fn(&T) -> AnyView + Send + Sync>;

/// One column of a [`DataGrid`](super::DataGrid).
///
/// Cheap to clone: the two closures are behind `Arc`, so a configuration can be
/// handed to the grid, the toolbar and the export without being rebuilt.
pub struct Column<T: 'static> {
    pub(crate) field: &'static str,
    /// What the column is called. A `String`, unlike `field`: the field name
    /// is machinery that goes in a sort parameter, the header is a word.
    pub(crate) header: String,
    pub(crate) searchable: bool,
    pub(crate) sortable: bool,
    pub(crate) hideable: bool,
    /// Off until someone turns it on in the column menu. For the detail a few
    /// people need and everyone else would have to scroll past.
    pub(crate) hidden_by_default: bool,
    /// Whether this column survives a phone-width screen.
    ///
    /// Distinct from [`hideable`](Self::hideable), which is about the column
    /// menu, and from [`hidden_by_default`](Self::hidden_by_default), which is
    /// about the viewer's choice. This one is about the screen: below `sm`
    /// there is room for two or three columns, and a table that insists on
    /// seven does not become readable by being scrollable.
    pub(crate) essential: bool,
    pub(crate) align: Align,
    /// Extra classes for this column's cells - a width, a whitespace rule.
    pub(crate) class: &'static str,
    pub(crate) read: Read<T>,
    pub(crate) draw: Option<Draw<T>>,
}

impl<T: 'static> Clone for Column<T> {
    fn clone(&self) -> Self {
        Self {
            field: self.field,
            header: self.header.clone(),
            searchable: self.searchable,
            sortable: self.sortable,
            hideable: self.hideable,
            hidden_by_default: self.hidden_by_default,
            essential: self.essential,
            align: self.align,
            class: self.class,
            read: Arc::clone(&self.read),
            draw: self.draw.clone(),
        }
    }
}

impl<T: 'static> Column<T> {
    /// A column that shows the value it reads, hideable, neither searchable
    /// nor sortable until it says so.
    pub fn new(
        field: &'static str,
        header: impl Into<String>,
        read: impl Fn(&T) -> Cell + Send + Sync + 'static,
    ) -> Self {
        Self {
            field,
            header: header.into(),
            searchable: false,
            sortable: false,
            hideable: true,
            essential: false,
            hidden_by_default: false,
            align: Align::Start,
            class: "",
            read: Arc::new(read),
            draw: None,
        }
    }

    /// The search box looks in this column.
    #[must_use]
    pub const fn searchable(mut self) -> Self {
        self.searchable = true;
        self
    }

    /// The heading can be clicked to sort by this column.
    #[must_use]
    pub const fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// searchable and sortable.
    #[must_use]
    pub const fn findable(self) -> Self {
        self.searchable().sortable()
    }

    /// Always on screen: not offered in the column menu.
    ///
    /// For the column that says which row this is. A table whose every column
    /// can be hidden can be turned into a grid of anonymous buttons.
    #[must_use]
    pub const fn pinned(mut self) -> Self {
        self.hideable = false;
        self
    }

    /// Present, but off until asked for.
    #[must_use]
    pub const fn hidden(mut self) -> Self {
        self.hidden_by_default = true;
        self
    }

    /// Kept on a phone, where most columns are dropped.
    ///
    /// Below `sm` only essential columns are drawn. Mark the one that says
    /// which row this is, and the one or two facts someone came to the screen
    /// to check - not more: three columns is what 390 pixels holds before the
    /// table starts scrolling sideways and taking the page with it.
    ///
    /// Nothing is lost by not being essential. The column is still exported,
    /// still searched, still sorted, and reappears the moment the screen is
    /// wide enough.
    #[must_use]
    pub const fn essential(mut self) -> Self {
        self.essential = true;
        self
    }

    #[must_use]
    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Extra Tailwind classes for this column's cells - typically a width.
    #[must_use]
    pub const fn class(mut self, class: &'static str) -> Self {
        self.class = class;
        self
    }

    /// Draw the cell as something other than its text.
    ///
    /// Changes appearance only. Searching, sorting and export still read the
    /// [`Cell`], which is what keeps a badge that says "Active" sorting under
    /// A.
    #[must_use]
    pub fn render(mut self, draw: impl Fn(&T) -> AnyView + Send + Sync + 'static) -> Self {
        self.draw = Some(Arc::new(draw));
        self
    }

    pub fn field(&self) -> &'static str {
        self.field
    }

    pub fn header(&self) -> &str {
        &self.header
    }

    /// What hides this column below `sm`, if anything.
    ///
    /// Done with a class rather than by leaving the cell out, so the server and
    /// the browser render the same table however wide the window is. A table
    /// whose *shape* depends on a media query is a table that cannot be
    /// hydrated: the browser would meet a row with fewer cells than the one it
    /// was sent.
    pub const fn responsive_class(&self) -> &'static str {
        if self.essential {
            ""
        } else {
            "hidden sm:table-cell"
        }
    }

    /// This column's value for one row.
    pub fn value(&self, row: &T) -> Cell {
        (self.read)(row)
    }

    /// This column's cell for one row, drawn.
    pub fn view(&self, row: &T) -> AnyView {
        match &self.draw {
            Some(draw) => draw(row),
            None => {
                let text = self.value(row).to_text();

                if text.is_empty() {
                    // An em dash rather than nothing: an empty cell and a cell
                    // whose value failed to load look identical otherwise.
                    view! { <span class="text-content-subtle">"—"</span> }.into_any()
                } else {
                    text.into_any()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Cell`'s own tests moved to `phonix_core::report::cell` with the type.
    // What is left here is what belongs to a column.

    #[test]
    fn a_renderer_changes_the_look_and_not_the_value() {
        let column = Column::new("status", "Status", |row: &&str| Cell::text(*row))
            .render(|_| ().into_any());

        assert_eq!(column.value(&"Active"), Cell::text("Active"));
    }
}
