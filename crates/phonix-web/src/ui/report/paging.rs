//! Which rows of a long report are on screen.
//!
//! The toolbar carries the controls and the sheet draws the rows, so the two
//! need one answer between them: the viewer provides this and the renderer
//! reads it.
//!
//! **This is the screen's pagination and not the page's.** A sheet somebody is
//! holding breaks where the paper runs out, which is what `@page` says and
//! what the browser works out when it prints. A report somebody is scrolling
//! breaks where reading stops being comfortable, which is a row count. They
//! are different questions and this answers the second - see ADR 0008 §8.

use leptos::prelude::*;

/// How many rows of a list report are read at once.
///
/// Ten, asked for directly. A report with fewer than this never draws a
/// control.
pub const ROWS_PER_PAGE: usize = 10;

/// Which page of a report is being read.
#[derive(Clone, Copy)]
pub struct Paging {
    /// Counted from one, the way a page number is.
    pub page: RwSignal<usize>,
    pub per_page: usize,
}

impl Paging {
    /// Make it available to the sheet below. The viewer calls this once.
    pub fn provide(per_page: usize) -> Self {
        let paging = Self {
            page: RwSignal::new(1),
            per_page,
        };

        provide_context(paging);
        paging
    }

    /// The paging in force, if the report is being read in a frame that has
    /// any. A report drawn outside one - the document settings preview - draws
    /// all of its rows.
    pub fn get() -> Option<Self> {
        use_context::<Self>()
    }

    /// How many pages that many rows come to. Never zero: an empty report is
    /// one empty page, because "page 1 of 0" is a page nobody can be on.
    pub fn pages(&self, rows: usize) -> usize {
        rows.div_ceil(self.per_page).max(1)
    }

    /// The rows this page shows, as the window into the report they are.
    pub fn window(&self) -> std::ops::Range<usize> {
        let page = self.page.get().max(1);
        let start = (page - 1) * self.per_page;

        start..start + self.per_page
    }
}
