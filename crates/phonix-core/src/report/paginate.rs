//! Where a page ends.
//!
//! A browser hands nobody page breaks and a PDF is nothing but pages, so this
//! is arithmetic over the look's own measurements rather than anything drawn:
//! a band's height in millimetres against what the sheet has left. The screen
//! and the writer read the same answer, which is the only way the page count
//! under the viewer and the pages in the file can agree.
//!
//! It works on a [`Rendered`] because that is the form both of them have.

use core::ops::Range;

use super::{Align, BandKind, Metrics, Rendered, RenderedBand};

/// Points in a millimetre. A page is measured in millimetres and type is
/// measured in points, and every report has both on it.
pub const PT_PER_MM: f32 = 72.0 / 25.4;

/// How far apart two stacked lines sit, as a share of their size. Read by the
/// writers, so a band is drawn in the lines it was measured for.
pub const LINE_SPACING: f32 = 1.35;

/// One band, or part of one, on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Which band of the report it is, by its place in [`Rendered::bands`].
    pub band: usize,
    pub kind: BandKind,
    /// The rows of that band drawn here. A band drawn once contributes its
    /// only row.
    pub rows: Range<usize>,
    /// Whether the headings are drawn above it.
    pub headings: bool,
    /// Drawn here because what it opened carries on, rather than because it
    /// starts here: a page header, a continued group, a continued detail band.
    pub repeated: bool,
}

/// One page of a report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrintedPage {
    pub pieces: Vec<Piece>,
}

impl PrintedPage {
    /// The rows of one band this page draws, if it draws any of it.
    pub fn rows_of(&self, band: usize) -> Option<Range<usize>> {
        self.pieces
            .iter()
            .find(|piece| piece.band == band)
            .map(|piece| piece.rows.clone())
    }
}

/// A report, in pages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pagination {
    pub pages: Vec<PrintedPage>,
}

impl Pagination {
    /// How many pages the report is.
    ///
    /// Never zero: a report with nothing in it is one empty page, because a
    /// viewer saying "page 1 of 0" is a screen nobody can be on.
    pub fn len(&self) -> usize {
        self.pages.len().max(1)
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// One page, counted from 1 the way a page number is.
    pub fn page(&self, number: usize) -> Option<&PrintedPage> {
        self.pages.get(number.checked_sub(1)?)
    }
}

/// What size a band's own text is set in.
///
/// Read by the writers as well as by the measuring below: a band measured at
/// one size and drawn at another is a page that overflows.
pub const fn text_size(kind: BandKind, metrics: &Metrics) -> f32 {
    match kind {
        BandKind::ReportHeader | BandKind::Detail => metrics.type_scale.body_pt,
        BandKind::PageHeader | BandKind::PageFooter => metrics.type_scale.caption_pt,
        BandKind::GroupHeader => metrics.type_scale.heading_pt,
        BandKind::GroupFooter | BandKind::ReportFooter => metrics.type_scale.total_pt,
    }
}

/// How tall a band is drawn, in millimetres.
///
/// What the look declares, unless what the band holds needs more: a letterhead
/// of six values stacked against one edge is six lines tall whatever the look
/// says a report header is.
pub fn band_height(band: &RenderedBand, metrics: &Metrics) -> f32 {
    let declared = metrics.bands.of(band.kind);

    if band.kind == BandKind::Detail || !stacks(band.kind) {
        return declared;
    }

    let line = text_size(band.kind, metrics) * LINE_SPACING / PT_PER_MM;
    let held = as_f32(stacked_lines(band)) * line + metrics.padding.vertical * 2.0;

    declared.max(held)
}

/// Whether a band of this kind stacks its values by the edge they sit against,
/// the way a letterhead does, rather than laying them out as one row of
/// columns.
pub const fn stacks(kind: BandKind) -> bool {
    !matches!(kind, BandKind::Detail | BandKind::GroupFooter)
}

/// How many lines the tallest of a stacked band's three groups comes to.
fn stacked_lines(band: &RenderedBand) -> usize {
    let cells = band.rows.first().map_or(0, Vec::len);

    [Align::Start, Align::Center, Align::End]
        .into_iter()
        .map(|edge| {
            (0..cells)
                .filter(|column| {
                    band.align(*column) == edge
                        && band
                            .rows
                            .first()
                            .and_then(|row| row.get(*column))
                            .is_some_and(|cell| !cell.is_empty())
                })
                .count()
        })
        .max()
        .unwrap_or_default()
}

/// Lay a report out in pages.
pub fn paginate(report: &Rendered) -> Pagination {
    let metrics = report.theme.metrics();
    let furniture = Furniture::of(report, &metrics);
    let available = (report.page.content_height_mm() - furniture.height).max(0.0);

    let letterhead = report
        .bands
        .iter()
        .any(|band| band.kind == BandKind::ReportHeader);

    let mut sheet = Sheet::new(&metrics, available, furniture, letterhead);

    for (index, band) in report.bands.iter().enumerate() {
        if sheet.furniture.owns(index) {
            continue;
        }

        match band.kind {
            BandKind::Detail => sheet.flow(index, band),
            kind => sheet.whole(index, kind, band_height(band, &metrics)),
        }
    }

    sheet.finish()
}

/// The bands that belong to the page rather than to the report, and the height
/// they take from every one of them.
struct Furniture {
    header: Option<usize>,
    footer: Option<usize>,
    height: f32,
}

impl Furniture {
    fn of(report: &Rendered, metrics: &Metrics) -> Self {
        let find = |kind: BandKind| report.bands.iter().position(|band| band.kind == kind);

        let header = find(BandKind::PageHeader);
        let footer = find(BandKind::PageFooter);

        let height = header.map_or(0.0, |_| metrics.bands.page_header)
            + footer.map_or(0.0, |_| metrics.bands.page_footer);

        Self {
            header,
            footer,
            height,
        }
    }

    fn owns(&self, band: usize) -> bool {
        self.header == Some(band) || self.footer == Some(band)
    }
}

/// The page being filled, and the ones already full.
struct Sheet<'a> {
    metrics: &'a Metrics,
    /// What one page has for the flow, with the furniture already taken off.
    available: f32,
    furniture: Furniture,
    pages: Vec<PrintedPage>,
    current: Vec<Piece>,
    used: f32,
    /// The group header still open, which is drawn again wherever its group
    /// carries on.
    group: Option<Piece>,
    /// Whether the page header is waiting for the letterhead to go above it,
    /// which is where the first page of a report puts the two.
    behind_the_letterhead: bool,
}

impl<'a> Sheet<'a> {
    fn new(metrics: &'a Metrics, available: f32, furniture: Furniture, letterhead: bool) -> Self {
        let mut sheet = Self {
            metrics,
            available,
            furniture,
            pages: Vec::new(),
            current: Vec::new(),
            used: 0.0,
            group: None,
            behind_the_letterhead: letterhead,
        };

        sheet.open();
        sheet
    }

    /// Start a page with its header on it.
    ///
    /// Except the first page of a report that has a letterhead: there the
    /// header goes under it, which is where the screen draws the two.
    fn open(&mut self) {
        if self.behind_the_letterhead && self.pages.is_empty() {
            return;
        }

        self.page_header(!self.pages.is_empty());
    }

    fn page_header(&mut self, repeated: bool) {
        if let Some(band) = self.furniture.header {
            self.current.push(Piece {
                band,
                kind: BandKind::PageHeader,
                rows: 0..1,
                headings: false,
                repeated,
            });
        }
    }

    /// How much of this page is left.
    fn room(&self) -> f32 {
        self.available - self.used
    }

    /// Whether nothing of the report itself has been placed here yet, which is
    /// what stops a band too tall for any page from turning pages for ever.
    fn fresh(&self) -> bool {
        !self.current.iter().any(|piece| !carried(piece))
    }

    /// Finish this page and start the next, carrying the open group's header
    /// onto it.
    fn turn(&mut self) {
        if let Some(band) = self.furniture.footer {
            self.current.push(Piece {
                band,
                kind: BandKind::PageFooter,
                rows: 0..1,
                headings: false,
                repeated: !self.pages.is_empty(),
            });
        }

        self.pages.push(PrintedPage {
            pieces: core::mem::take(&mut self.current),
        });
        self.used = 0.0;
        self.open();

        if let Some(group) = self.group.clone() {
            self.used += self.metrics.bands.group_header;
            self.current.push(Piece {
                repeated: true,
                ..group
            });
        }
    }

    /// A band drawn once: placed whole, or moved to the next page.
    fn whole(&mut self, band: usize, kind: BandKind, height: f32) {
        if height > self.room() && !self.fresh() {
            self.turn();
        }

        let piece = Piece {
            band,
            kind,
            rows: 0..1,
            headings: false,
            repeated: false,
        };

        self.used += height;
        self.current.push(piece.clone());

        if kind == BandKind::ReportHeader && self.behind_the_letterhead {
            self.behind_the_letterhead = false;
            self.page_header(false);
        }

        match kind {
            // A group header with no room for a row of its group under it is a
            // heading over nothing, so it goes to the next page instead.
            BandKind::GroupHeader => {
                if self.room() < self.metrics.bands.detail && !self.fresh() {
                    self.current.pop();
                    self.used -= height;
                    self.group = Some(piece);
                    self.turn();
                } else {
                    self.group = Some(piece);
                }
            }
            BandKind::GroupFooter => self.group = None,
            _ => {}
        }
    }

    /// The detail band, a page's worth of rows at a time.
    fn flow(&mut self, band: usize, rows: &RenderedBand) {
        let headed = !rows.headings.is_empty();
        let row = self.metrics.bands.detail;
        let mut placed = 0;

        while placed < rows.rows.len() {
            // The headings are drawn again wherever the band carries on, so a
            // page of rows is never a page of unlabelled figures.
            let headings = if headed { row } else { 0.0 };
            let fits = fitting(self.room() - headings, row);

            if fits == 0 && !self.fresh() {
                self.turn();
                continue;
            }

            let take = fits.max(1).min(rows.rows.len() - placed);

            self.used += headings + row * as_f32(take);
            self.current.push(Piece {
                band,
                kind: BandKind::Detail,
                rows: placed..placed + take,
                headings: headed,
                repeated: placed > 0,
            });

            placed += take;
        }
    }

    fn finish(mut self) -> Pagination {
        self.turn();

        Pagination { pages: self.pages }
    }
}

/// Whether a piece is what a page arrives with rather than something placed on
/// it: the page header, and the header of a group that carried on.
fn carried(piece: &Piece) -> bool {
    piece.kind == BandKind::PageHeader || (piece.kind == BandKind::GroupHeader && piece.repeated)
}

/// How many rows of this height fit in this much room.
fn fitting(room: f32, row: f32) -> usize {
    if room <= 0.0 || row <= 0.0 {
        return 0;
    }

    let fits = (room / row).floor();

    if fits <= 0.0 { 0 } else { fits as usize }
}

/// A count as a length, which is the one place this module leaves the integers.
///
/// A count too large for a `u16` is a report of sixty-five thousand rows on
/// one page, which the arithmetic above has already ruled out.
fn as_f32(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::MAX, f32::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{PageSetup, ReportTheme};

    fn report(theme: ReportTheme, bands: Vec<RenderedBand>) -> Rendered {
        Rendered {
            report_id: "test".to_owned(),
            title: "Test".to_owned(),
            theme,
            page: PageSetup::default(),
            bands,
        }
    }

    fn rows(count: usize) -> Vec<Vec<String>> {
        (0..count).map(|row| vec![row.to_string()]).collect()
    }

    fn detail(count: usize) -> RenderedBand {
        RenderedBand::table(BandKind::Detail, vec!["Name".to_owned()], rows(count))
    }

    /// Every row of the band, page by page, as one sequence.
    fn drawn(pagination: &Pagination, band: usize) -> Vec<usize> {
        pagination
            .pages
            .iter()
            .flat_map(|page| page.pieces.iter())
            .filter(|piece| piece.band == band)
            .flat_map(|piece| piece.rows.clone())
            .collect()
    }

    #[test]
    fn a_band_is_measured_at_what_it_holds() {
        // A letterhead of four values against one edge is four lines tall,
        // whatever a report header is declared to be.
        let metrics = ReportTheme::Modern.metrics();
        let letterhead = RenderedBand::once(
            BandKind::ReportHeader,
            (0..8).map(|line| format!("Line {line}")).collect(),
        )
        .aligned(vec![Align::Start; 8]);

        assert!(
            band_height(&letterhead, &metrics) > metrics.bands.report_header,
            "eight lines were measured at the height of one band",
        );
        assert_eq!(
            band_height(&detail(3), &metrics),
            metrics.bands.detail,
            "a detail band is one row tall however many rows it holds",
        );
    }

    #[test]
    fn a_short_report_is_one_page() {
        let pagination = paginate(&report(ReportTheme::Modern, vec![detail(5)]));

        assert_eq!(pagination.len(), 1);
        assert_eq!(drawn(&pagination, 0), (0..5).collect::<Vec<_>>());
    }

    #[test]
    fn every_row_is_drawn_exactly_once() {
        let pagination = paginate(&report(ReportTheme::Modern, vec![detail(250)]));

        assert!(pagination.len() > 1, "250 rows is more than one page");
        assert_eq!(drawn(&pagination, 0), (0..250).collect::<Vec<_>>());
    }

    #[test]
    fn the_headings_are_drawn_again_wherever_the_band_carries_on() {
        let pagination = paginate(&report(ReportTheme::Modern, vec![detail(250)]));

        for page in &pagination.pages {
            let detail = page
                .pieces
                .iter()
                .find(|piece| piece.kind == BandKind::Detail);

            if let Some(piece) = detail {
                assert!(piece.headings, "a page of rows with no headings over them");
            }
        }
    }

    #[test]
    fn a_page_header_is_on_every_page() {
        let pagination = paginate(&report(
            ReportTheme::Modern,
            vec![
                RenderedBand::once(BandKind::PageHeader, vec!["Product list".to_owned()]),
                detail(250),
            ],
        ));

        for page in &pagination.pages {
            assert!(
                page.pieces
                    .iter()
                    .any(|piece| piece.kind == BandKind::PageHeader),
                "a page with no header on it",
            );
        }
    }

    #[test]
    fn the_first_page_puts_its_letterhead_above_the_page_header() {
        // Where the screen draws the two, and a file that disagreed would not
        // be the same document.
        let pagination = paginate(&report(
            ReportTheme::Modern,
            vec![
                RenderedBand::once(BandKind::ReportHeader, vec!["As at".to_owned()]),
                RenderedBand::once(BandKind::PageHeader, vec!["Product list".to_owned()]),
                detail(400),
            ],
        ));

        let first: Vec<BandKind> = pagination.pages[0]
            .pieces
            .iter()
            .map(|piece| piece.kind)
            .collect();

        assert_eq!(
            &first[..2],
            &[BandKind::ReportHeader, BandKind::PageHeader],
            "the letterhead is not above the page header",
        );
        assert_eq!(
            pagination.pages[1].pieces.first().map(|piece| piece.kind),
            Some(BandKind::PageHeader),
            "a later page starts with anything but its header",
        );
    }

    #[test]
    fn a_group_header_alone_at_the_foot_of_a_page_moves_to_the_next_one() {
        let theme = ReportTheme::Modern;
        // Rows enough to leave room for the header and not for a row under it.
        let pagination = paginate(&report(
            theme,
            vec![
                detail(orphaning_rows(theme)),
                RenderedBand::once(BandKind::GroupHeader, vec!["Seating".to_owned()]),
                detail(3),
            ],
        ));

        let first = &pagination.pages[0];

        assert!(
            !first
                .pieces
                .iter()
                .any(|piece| piece.kind == BandKind::GroupHeader),
            "the header was left at the foot of a page with none of its group",
        );
        assert!(
            pagination.pages[1]
                .pieces
                .iter()
                .any(|piece| piece.kind == BandKind::GroupHeader),
            "the header did not move to its group",
        );
    }

    /// How many rows leave a page with room for a group header but not for a
    /// row of the group under it.
    fn orphaning_rows(theme: ReportTheme) -> usize {
        let metrics = theme.metrics();
        let available = PageSetup::default().content_height_mm();

        (1..500)
            .find(|rows| {
                let room = available - metrics.bands.detail * f32::from(*rows as u16 + 1);

                room >= metrics.bands.group_header
                    && room - metrics.bands.group_header < metrics.bands.detail
            })
            .expect("a look whose bands leave such a page")
    }

    #[test]
    fn a_continued_group_draws_its_header_again() {
        let theme = ReportTheme::Modern;
        let pagination = paginate(&report(
            theme,
            vec![
                RenderedBand::once(BandKind::GroupHeader, vec!["Seating".to_owned()]),
                detail(rows_per_page(theme) * 2),
                RenderedBand::once(BandKind::GroupFooter, vec!["Subtotal".to_owned()]),
            ],
        ));

        let repeats = pagination
            .pages
            .iter()
            .skip(1)
            .filter(|page| {
                page.pieces
                    .iter()
                    .any(|piece| piece.kind == BandKind::GroupHeader && piece.repeated)
            })
            .count();

        assert!(repeats > 0, "the group carried on with nothing to say so");
    }

    #[test]
    fn a_row_taller_than_a_page_is_drawn_rather_than_dropped() {
        // Margins that leave less room than one row is tall.
        let mut narrow = report(ReportTheme::Modern, vec![detail(3)]);
        narrow.page.margins = crate::report::Margins::uniform(145.0);

        let pagination = paginate(&narrow);

        assert_eq!(drawn(&pagination, 0), vec![0, 1, 2]);
        assert_eq!(pagination.len(), 3, "one row a page, and no page lost");
    }

    #[test]
    fn a_band_that_exactly_fills_a_page_does_not_start_another() {
        let theme = ReportTheme::Modern;
        let pagination = paginate(&report(theme, vec![detail(rows_per_page(theme))]));

        assert_eq!(pagination.len(), 1);
    }

    #[test]
    fn compact_fits_more_rows_on_a_page_than_modern() {
        // The one assertion that proves the look reaches the arithmetic rather
        // than only the drawing.
        assert!(rows_per_page(ReportTheme::Compact) > rows_per_page(ReportTheme::Modern));
    }

    /// How many rows of a headed detail band one page of this look holds.
    fn rows_per_page(theme: ReportTheme) -> usize {
        let metrics = theme.metrics();
        let room = PageSetup::default().content_height_mm() - metrics.bands.detail;

        fitting(room, metrics.bands.detail)
    }
}
