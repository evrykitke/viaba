//! The report as a PDF.
//!
//! The pages come from [`phonix_core::report::paginate`] and the measurements
//! from the look's own metrics, so the file and the screen are one document
//! laid out twice rather than two documents that resemble each other.
//!
//! # No font is embedded
//!
//! There is no typeface in this repository, so text is written in Helvetica -
//! one of the fourteen faces every reader has - encoded as Windows-1252. That
//! carries English and the Latin-1 languages and nothing else. A report in a
//! script it cannot carry **fails by name** rather than writing a file of
//! blank boxes, which is a file somebody would send to a customer.
//!
//! Embedding a face is what lifts that, and it is the only thing in here that
//! would have to change.

use core::fmt;

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
use phonix_core::report::{
    Align, BandKind, LINE_SPACING, Metrics, PT_PER_MM, PrintedPage, Rendered, RenderedBand,
    band_height, paginate, stacks, text_size,
};

/// What the cap of a letter comes to, as a share of its size. Used to sit a
/// line of text in the middle of the band it belongs to.
const CAP_HEIGHT: f32 = 0.7;

const REGULAR: Name<'static> = Name(b"F1");
const BOLD: Name<'static> = Name(b"F2");

/// What stopped a report becoming a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// Text the base-14 encoding cannot carry - Chinese, Greek, anything
    /// outside Latin-1.
    Unwritable { text: String },
}

impl fmt::Display for PdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unwritable { text } => write!(
                formatter,
                "this report cannot be written as a PDF until a font is embedded: \
                 `{text}` is outside the encoding the built-in faces carry",
            ),
        }
    }
}

impl std::error::Error for PdfError {}

/// The report as a PDF.
pub fn to_pdf(report: &Rendered) -> Result<Vec<u8>, PdfError> {
    let metrics = report.theme.metrics();
    let pagination = paginate(report);

    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let regular = Ref::new(3);
    let bold = Ref::new(4);

    let mut drawn: Vec<(Ref, Ref, Vec<u8>)> = Vec::new();
    let mut next = 5_i32;

    for page in &pagination.pages {
        let page_id = Ref::new(next);
        let content_id = Ref::new(next + 1);
        next += 2;

        drawn.push((page_id, content_id, draw(report, &metrics, page)?));
    }

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(drawn.iter().map(|(page, _, _)| *page))
        .count(i32::try_from(drawn.len()).unwrap_or(i32::MAX));

    let box_ = Rect::new(
        0.0,
        0.0,
        report.page.width_mm() * PT_PER_MM,
        report.page.height_mm() * PT_PER_MM,
    );

    for (page_id, content_id, stream) in &drawn {
        let mut page = pdf.page(*page_id);
        page.parent(tree);
        page.media_box(box_);
        page.contents(*content_id);
        page.resources()
            .fonts()
            .pair(REGULAR, regular)
            .pair(BOLD, bold);
        page.finish();

        pdf.stream(*content_id, stream);
    }

    // Base-14, so no font data is written and no licence travels with the
    // file. WinAnsi rather than the standard encoding, which has no accents in
    // it at all.
    pdf.type1_font(regular)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.type1_font(bold)
        .base_font(Name(b"Helvetica-Bold"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    Ok(pdf.finish())
}

/// The content stream for one page.
fn draw(report: &Rendered, metrics: &Metrics, page: &PrintedPage) -> Result<Vec<u8>, PdfError> {
    let mut content = Content::new();

    let height = report.page.height_mm() * PT_PER_MM;
    let left = report.page.margins.left * PT_PER_MM;
    let right = (report.page.width_mm() - report.page.margins.right) * PT_PER_MM;
    let foot = report.page.margins.bottom * PT_PER_MM;

    let mut top = height - report.page.margins.top * PT_PER_MM;

    for piece in &page.pieces {
        let Some(band) = report.bands.get(piece.band) else {
            continue;
        };

        let column = Columns {
            left,
            right,
            padding: metrics.padding.horizontal * PT_PER_MM,
        };

        match piece.kind {
            // The foot of the sheet, not the foot of the flow.
            BandKind::PageFooter => {
                let tall = band_height(band, metrics) * PT_PER_MM;

                stacked(&mut content, band, foot + tall, metrics, column)?;
            }
            BandKind::Detail => {
                if piece.headings {
                    let heading = Line {
                        top,
                        height: metrics.bands.detail * PT_PER_MM,
                        size: metrics.type_scale.heading_pt,
                        bold: true,
                    };

                    row(&mut content, band, &band.headings, &heading, column)?;
                    top -= heading.height;
                    rule(&mut content, left, right, top, metrics.rules.under_headings);
                }

                for cells in band.rows.get(piece.rows.clone()).unwrap_or_default() {
                    let line = Line {
                        top,
                        height: metrics.bands.detail * PT_PER_MM,
                        size: metrics.type_scale.body_pt,
                        bold: false,
                    };

                    row(&mut content, band, cells, &line, column)?;
                    top -= line.height;
                    rule(&mut content, left, right, top, metrics.rules.between_rows);
                }
            }
            kind => {
                let tall = band_height(band, metrics) * PT_PER_MM;

                if matches!(kind, BandKind::ReportFooter | BandKind::GroupFooter) {
                    rule(&mut content, left, right, top, metrics.rules.above_total);
                }

                if kind == BandKind::ReportHeader {
                    let title = Line {
                        top,
                        height: metrics.type_scale.title_pt * 1.6,
                        size: metrics.type_scale.title_pt,
                        bold: true,
                    };

                    text(
                        &mut content,
                        &report.title,
                        left,
                        title.baseline(),
                        title.size,
                        true,
                    )?;
                    top -= title.height;
                }

                if stacks(kind) {
                    stacked(&mut content, band, top, metrics, column)?;
                } else {
                    // A subtotal is a row of the same columns as the rows it
                    // totals, so its figures sit under them.
                    let line = Line {
                        top,
                        height: tall,
                        size: text_size(kind, metrics),
                        bold: true,
                    };

                    row(
                        &mut content,
                        band,
                        &band.rows.first().cloned().unwrap_or_default(),
                        &line,
                        column,
                    )?;
                }

                top -= tall;

                if kind == BandKind::ReportHeader {
                    rule(
                        &mut content,
                        left,
                        right,
                        top,
                        metrics.rules.under_letterhead,
                    );
                }
            }
        }
    }

    Ok(content.finish().into_vec())
}

/// Where a row of cells sits, and how it is set.
struct Line {
    /// The top edge of the band, measured from the foot of the sheet.
    top: f32,
    height: f32,
    size: f32,
    bold: bool,
}

impl Line {
    /// Where the letters stand, which is the middle of the band rather than
    /// its edge.
    fn baseline(&self) -> f32 {
        self.top - (self.height + self.size * CAP_HEIGHT) / 2.0
    }
}

/// The columns a row is laid out in: equal shares of the content width, the
/// way the screen divides them.
#[derive(Clone, Copy)]
struct Columns {
    left: f32,
    right: f32,
    padding: f32,
}

impl Columns {
    fn edges(&self, column: usize, of: usize) -> (f32, f32) {
        let width = (self.right - self.left) / at_least_one(of);
        let start = self.left + width * as_f32(column);

        (start + self.padding, start + width - self.padding)
    }
}

/// One row of cells, each under the column it belongs to.
fn row(
    content: &mut Content,
    band: &RenderedBand,
    cells: &[String],
    line: &Line,
    columns: Columns,
) -> Result<(), PdfError> {
    let baseline = line.baseline();

    for (index, cell) in cells.iter().enumerate() {
        if cell.is_empty() {
            continue;
        }

        let (start, end) = columns.edges(index, cells.len());
        let cell = fitted(cell, line.size, end - start);

        let x = match band.align(index) {
            Align::Start => start,
            Align::Center => (start + end - width_of(&cell, line.size)) / 2.0,
            Align::End => end - width_of(&cell, line.size),
        };

        text(content, &cell, x, baseline, line.size, line.bold)?;
    }

    Ok(())
}

/// A band drawn once: its values stacked against the edge each sits on.
///
/// What the screen does with a letterhead - three groups, one per edge, each a
/// column of lines - rather than one row of equal columns, which prints four
/// values on top of each other. Each group is given the width it needs rather
/// than a third of the page, so a long note beside a short figure still reads.
fn stacked(
    content: &mut Content,
    band: &RenderedBand,
    top: f32,
    metrics: &Metrics,
    columns: Columns,
) -> Result<(), PdfError> {
    let cells = band.rows.first().cloned().unwrap_or_default();
    let size = text_size(band.kind, metrics);
    let bold = matches!(band.kind, BandKind::GroupHeader | BandKind::ReportFooter);
    let step = size * LINE_SPACING;
    let first = top - metrics.padding.vertical * PT_PER_MM - size * CAP_HEIGHT;

    let group = |edge: Align| -> Vec<String> {
        cells
            .iter()
            .enumerate()
            .filter(|(column, cell)| !cell.is_empty() && band.align(*column) == edge)
            .map(|(_, cell)| cell.clone())
            .collect()
    };

    let start = group(Align::Start);
    let centre = group(Align::Center);
    let end = group(Align::End);

    // Inside the same padding a cell of the detail band has, so a letterhead
    // and the headings under it start on the same line down the page.
    let left = columns.left + columns.padding;
    let right = columns.right - columns.padding;

    let gap = columns.padding * 2.0;
    let end_width = widest(&end, size);
    let centre_width = widest(&centre, size);
    let start_width = (right - left - end_width - centre_width - gap * 2.0).max(gap);

    for (index, line) in start.iter().enumerate() {
        let line = fitted(line, size, start_width);

        text(
            content,
            &line,
            left,
            first - step * as_f32(index),
            size,
            bold,
        )?;
    }

    for (index, line) in centre.iter().enumerate() {
        let line = fitted(line, size, centre_width.max(gap));
        let middle = (left + right - width_of(&line, size)) / 2.0;

        text(
            content,
            &line,
            middle,
            first - step * as_f32(index),
            size,
            bold,
        )?;
    }

    for (index, line) in end.iter().enumerate() {
        let line = fitted(line, size, end_width.max(gap));

        text(
            content,
            &line,
            right - width_of(&line, size),
            first - step * as_f32(index),
            size,
            bold,
        )?;
    }

    Ok(())
}

/// The widest of a group's lines.
fn widest(lines: &[String], size: f32) -> f32 {
    lines
        .iter()
        .map(|line| width_of(line, size))
        .fold(0.0_f32, f32::max)
}

/// A line cut to the width it has, rather than drawn across its neighbour.
///
/// The screen wraps instead, which is what a `<div>` does for nothing; wrapping
/// here would change how tall a band is after the paginator has already said
/// where the page ends.
fn fitted(line: &str, size: f32, width: f32) -> String {
    if width_of(line, size) <= width {
        return line.to_owned();
    }

    let mut cut = String::new();
    let room = width - width_of("\u{2026}", size);

    for letter in line.chars() {
        if width_of(&cut, size) + width_of(&letter.to_string(), size) > room {
            break;
        }

        cut.push(letter);
    }

    cut.push('\u{2026}');
    cut
}

/// One run of text./// One run of text.
fn text(
    content: &mut Content,
    words: &str,
    x: f32,
    baseline: f32,
    size: f32,
    bold: bool,
) -> Result<(), PdfError> {
    let encoded = encoded(words)?;

    content.begin_text();
    content.set_font(if bold { BOLD } else { REGULAR }, size);
    content.next_line(x, baseline);
    content.show(Str(&encoded));
    content.end_text();

    Ok(())
}

/// A rule across the content width, drawn as the thin box it is.
fn rule(content: &mut Content, left: f32, right: f32, y: f32, weight_mm: f32) {
    if weight_mm <= 0.0 {
        return;
    }

    content.set_fill_gray(0.35);
    content.rect(left, y, right - left, weight_mm * PT_PER_MM);
    content.fill_nonzero();
    content.set_fill_gray(0.0);
}

/// Text in the encoding the built-in faces carry, or a refusal naming it.
fn encoded(words: &str) -> Result<Vec<u8>, PdfError> {
    words
        .chars()
        .map(win_ansi)
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| PdfError::Unwritable {
            text: words.to_owned(),
        })
}

/// One character in Windows-1252, if it has one.
///
/// Latin-1 is the code page below 0x100 with a hole in it where Windows puts
/// its punctuation; the eight characters worth carrying from that hole are
/// here, and everything else outside Latin-1 is a refusal.
fn win_ansi(letter: char) -> Option<u8> {
    match letter {
        '\u{2018}' => Some(0x91),
        '\u{2019}' => Some(0x92),
        '\u{201c}' => Some(0x93),
        '\u{201d}' => Some(0x94),
        '\u{2013}' => Some(0x96),
        '\u{2014}' => Some(0x97),
        '\u{2026}' => Some(0x85),
        '\u{20ac}' => Some(0x80),
        // The C1 block is where Windows-1252 differs from Latin-1, so a
        // character that lands there is not the one it looks like.
        letter if ('\u{a0}'..='\u{ff}').contains(&letter) || letter.is_ascii() => {
            u8::try_from(u32::from(letter)).ok()
        }
        _ => None,
    }
}

/// How wide a run of text is, in points.
///
/// Helvetica's own widths for the characters a figure is made of, which are
/// the ones that have to line up, and its average for the rest. Exact widths
/// arrive with an embedded font; until then a right-aligned column of money is
/// right and a right-aligned sentence is close.
fn width_of(words: &str, size: f32) -> f32 {
    let em: f32 = words
        .chars()
        .map(|letter| match letter {
            '0'..='9' => 0.556,
            '.' | ',' | ' ' | '\'' => 0.278,
            '-' => 0.333,
            '(' | ')' => 0.333,
            _ => 0.52,
        })
        .sum();

    em * size
}

/// A count as a length, for laying columns out.
fn as_f32(count: usize) -> f32 {
    u16::try_from(count).map_or(f32::MAX, f32::from)
}

/// A column count that cannot divide by zero.
fn at_least_one(count: usize) -> f32 {
    as_f32(count.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonix_core::report::{PageSetup, ReportTheme};

    fn report(bands: Vec<RenderedBand>) -> Rendered {
        Rendered {
            report_id: "test".to_owned(),
            title: "Product list".to_owned(),
            theme: ReportTheme::Modern,
            page: PageSetup::default(),
            bands,
        }
    }

    fn detail(rows: usize) -> RenderedBand {
        RenderedBand::table(
            BandKind::Detail,
            vec!["Name".to_owned(), "Cost".to_owned()],
            (0..rows)
                .map(|row| vec![format!("Item {row}"), format!("{row}.00")])
                .collect(),
        )
        .aligned(vec![Align::Start, Align::End])
    }

    #[test]
    fn a_report_becomes_a_pdf() {
        let bytes = to_pdf(&report(vec![detail(5)])).expect("a Latin-1 report");

        assert!(bytes.starts_with(b"%PDF-1."), "not a PDF");
        assert!(bytes.ends_with(b"%%EOF\n") || bytes.ends_with(b"%%EOF"));
    }

    #[test]
    fn a_long_report_is_written_in_pages() {
        let one = to_pdf(&report(vec![detail(5)])).expect("a short report");
        let many = to_pdf(&report(vec![detail(400)])).expect("a long report");

        assert!(
            count_of(&many, b"/Type /Page\n") > count_of(&one, b"/Type /Page\n"),
            "400 rows is more than one page",
        );
    }

    #[test]
    fn a_script_the_built_in_faces_cannot_carry_is_refused() {
        let refused = to_pdf(&report(vec![RenderedBand::once(
            BandKind::ReportHeader,
            vec!["\u{4ea7}\u{54c1}\u{6e05}\u{5355}".to_owned()],
        )]));

        assert!(
            matches!(refused, Err(PdfError::Unwritable { .. })),
            "a file of blank boxes was written instead of a refusal",
        );
    }

    #[test]
    fn accents_are_written_rather_than_refused() {
        let written = to_pdf(&report(vec![RenderedBand::once(
            BandKind::ReportHeader,
            vec!["Rélevé de compte - 1 200,00 €".to_owned()],
        )]));

        assert!(written.is_ok(), "Latin-1 is inside the encoding");
    }

    #[test]
    fn a_letterhead_stacks_rather_than_overlapping() {
        // Four values on one baseline in four columns is what the first
        // exported statement did, and all four sat on top of each other.
        let letterhead = RenderedBand::once(
            BandKind::ReportHeader,
            vec![
                "jamo101".to_owned(),
                "2026-01-01 to 2026-09-16".to_owned(),
                "Every figure is in USD.".to_owned(),
                "Opening balance 0.00".to_owned(),
            ],
        )
        .aligned(vec![Align::Start, Align::Start, Align::Start, Align::End]);

        let bytes = to_pdf(&report(vec![letterhead])).expect("a letterhead");
        let baselines = baselines_of(&bytes);

        assert_eq!(baselines.len(), 5, "a title and four values");
        assert_eq!(
            baselines
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4,
            "the three values against one edge are not on three lines",
        );
    }

    /// The line every run of text stands on, as it is written in the file.
    fn baselines_of(bytes: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter(|line| line.ends_with(" Td"))
            .filter_map(|line| line.split_whitespace().nth(1).map(str::to_owned))
            .collect()
    }

    fn count_of(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    }
}
