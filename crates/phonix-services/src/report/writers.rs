//! Turning a rendered report into bytes.
//!
//! A writer is a plain function: a [`Rendered`] in, bytes out. No pool, no
//! worker context, no caller. That is what lets the request path use it for a
//! bounded report and the exporter use it for one that is not - ADR 0008 §9,
//! and the reason a receipt and a statement cannot disagree about what a CSV
//! looks like.

use phonix_core::report::Rendered;

/// The report as a CSV.
///
/// Every band is written, in the order the report drew them, so a statement's
/// opening balance and its four totals are in the file rather than only its
/// lines. A band drawn once contributes its own row; the detail band
/// contributes its headings and then its lines.
///
/// There is no chart band yet. When there is, it skips itself here rather
/// than being flattened into rows: numbers pretending to be data, in a file
/// somebody is about to sum, is the one thing a chart must not become.
pub fn to_csv(report: &Rendered) -> String {
    let width = report.width();
    let mut out = String::new();

    for band in &report.bands {
        if !band.headings.is_empty() {
            write_row(&mut out, &band.headings, width);
        }

        for row in &band.rows {
            write_row(&mut out, row, width);
        }
    }

    out
}

/// One row, padded to the report's own width so every line has the same number
/// of commas - a spreadsheet reading a short row puts the totals under the
/// wrong headings.
fn write_row(out: &mut String, cells: &[String], width: usize) {
    for column in 0..width {
        if column > 0 {
            out.push(',');
        }

        if let Some(cell) = cells.get(column) {
            out.push_str(&escaped(cell));
        }
    }

    out.push_str("\r\n");
}

/// One cell, in the form a spreadsheet reads back as what it says.
///
/// Quoted when it holds a comma, a quote or a newline, with inner quotes
/// doubled - RFC 4180. And quoted when it *starts* with one of the four
/// characters a spreadsheet treats as the beginning of a formula: a cell
/// reading `=1+1` is a document that computes something when somebody opens
/// it, which is the injection every CSV export has to answer for.
fn escaped(cell: &str) -> String {
    let risky = cell.contains([',', '"', '\n', '\r']);
    let formula = cell.starts_with(['=', '+', '-', '@']);

    if !risky && !formula {
        return cell.to_owned();
    }

    format!("\"{}\"", cell.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use phonix_core::report::{BandKind, PageSetup, RenderedBand, ReportTheme};

    use super::*;

    fn report(bands: Vec<RenderedBand>) -> Rendered {
        Rendered {
            report_id: "test".to_owned(),
            title: "Test".to_owned(),
            theme: ReportTheme::Compact,
            page: PageSetup::default(),
            bands,
        }
    }

    fn rows(csv: &str) -> Vec<&str> {
        csv.trim_end_matches("\r\n").split("\r\n").collect()
    }

    #[test]
    fn a_detail_band_writes_its_headings_and_then_its_lines() {
        let csv = to_csv(&report(vec![RenderedBand::table(
            BandKind::Detail,
            vec!["Code".to_owned(), "Name".to_owned()],
            vec![
                vec!["A1".to_owned(), "Sofa".to_owned()],
                vec!["A2".to_owned(), "Lamp".to_owned()],
            ],
        )]));

        assert_eq!(rows(&csv), vec!["Code,Name", "A1,Sofa", "A2,Lamp"]);
    }

    #[test]
    fn every_row_is_the_width_of_the_widest_band() {
        let csv = to_csv(&report(vec![
            RenderedBand::table(
                BandKind::Detail,
                vec!["A".to_owned(), "B".to_owned(), "C".to_owned()],
                vec![vec!["1".to_owned(), "2".to_owned(), "3".to_owned()]],
            ),
            // Two cells where the report is three wide: a short row would put
            // this total under the wrong heading.
            RenderedBand::once(
                BandKind::ReportFooter,
                vec!["Total".to_owned(), "6".to_owned()],
            ),
        ]));

        assert_eq!(rows(&csv).last(), Some(&"Total,6,"));
    }

    #[test]
    fn the_four_things_that_break_a_csv_are_quoted() {
        let csv = to_csv(&report(vec![RenderedBand::once(
            BandKind::Detail,
            vec![
                "Smith, Jane".to_owned(),
                "the \"good\" one".to_owned(),
                "two\nlines".to_owned(),
                "=1+1".to_owned(),
            ],
        )]));

        assert_eq!(
            csv,
            "\"Smith, Jane\",\"the \"\"good\"\" one\",\"two\nlines\",\"=1+1\"\r\n"
        );
    }
}
