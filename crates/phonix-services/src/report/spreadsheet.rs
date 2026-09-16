//! The report as a spreadsheet.
//!
//! The format an accounts department actually asks for, and the reason the
//! cells are typed: a CSV hands everything over as text, so the column
//! somebody wanted to total is a column of words and the dates sort as
//! strings. Here a number is a number, an amount is a number with a currency
//! format on it, and a date is a date.
//!
//! A writer, not a printer - the PDF is the report's own page and this is not.
//! See ADR 0008 §9.

use phonix_core::report::{BandKind, Cell, Rendered};
use rust_xlsxwriter::{Format, FormatBorder, Workbook, XlsxError};

/// How wide a column is, in characters. Wide enough for a date and a figure
/// with a currency on it, which are the two things that get truncated.
const COLUMN_WIDTH: f64 = 18.0;

/// The report as an XLSX.
pub fn to_xlsx(report: &Rendered) -> Result<Vec<u8>, XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();

    sheet.set_name(tab_name(&report.title))?;

    let title = Format::new().set_bold().set_font_size(14);
    let heading = Format::new()
        .set_bold()
        .set_border_bottom(FormatBorder::Thin);
    let group = Format::new().set_bold();
    let total = Format::new()
        .set_bold()
        .set_border_top(FormatBorder::Thin)
        .set_num_format(MONEY);
    let money = Format::new().set_num_format(MONEY);
    let date = Format::new().set_num_format("yyyy-mm-dd hh:mm");

    sheet.write_string_with_format(0, 0, &report.title, &title)?;

    let mut row = 2_u32;
    // Where the rows start, which is what is frozen: the headings stay on
    // screen while somebody scrolls a thousand lines.
    let mut headed = None;

    for band in &report.bands {
        if !band.headings.is_empty() {
            for (column, label) in band.headings.iter().enumerate() {
                sheet.write_string_with_format(row, column as u16, label, &heading)?;
                sheet.set_column_width(column as u16, COLUMN_WIDTH)?;
            }

            row += 1;
            headed = headed.or(Some(row));
        }

        let emphasis = match band.kind {
            BandKind::GroupHeader => Some(&group),
            BandKind::GroupFooter | BandKind::ReportFooter => Some(&total),
            _ => None,
        };

        for cells in &band.rows {
            for (column, cell) in cells.iter().enumerate() {
                write(sheet, row, column as u16, cell, emphasis, &money, &date)?;
            }

            row += 1;
        }

        // A blank line under a band that closes something, so the sections a
        // reader sees on the page are the sections in the file.
        if matches!(band.kind, BandKind::GroupFooter | BandKind::ReportFooter) {
            row += 1;
        }
    }

    if let Some(headed) = headed {
        sheet.set_freeze_panes(headed, 0)?;
    }

    workbook.save_to_buffer()
}

/// The currency format. Two decimals and a thousands separator, without a
/// symbol: the report says what currency it is in, and a symbol here would be
/// the wrong one as often as the right one.
const MONEY: &str = "#,##0.00";

/// One cell, as the thing it is.
fn write(
    sheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    column: u16,
    cell: &Cell,
    emphasis: Option<&Format>,
    money: &Format,
    date: &Format,
) -> Result<(), XlsxError> {
    match cell {
        Cell::Empty => {}
        Cell::Money(amount) => {
            let format = emphasis.unwrap_or(money);

            sheet.write_number_with_format(row, column, as_number(amount), format)?;
        }
        Cell::Number(value) => match emphasis {
            Some(format) => {
                sheet.write_number_with_format(row, column, *value, format)?;
            }
            None => {
                sheet.write_number(row, column, *value)?;
            }
        },
        Cell::Timestamp(at) => {
            sheet.write_datetime_with_format(row, column, at.naive_utc(), date)?;
        }
        other => match emphasis {
            Some(format) => {
                sheet.write_string_with_format(row, column, other.to_text(), format)?;
            }
            None => {
                sheet.write_string(row, column, other.to_text())?;
            }
        },
    }

    Ok(())
}

/// An amount as the number a spreadsheet adds up.
fn as_number(amount: &phonix_core::money::Money) -> f64 {
    amount.to_display_string().parse().unwrap_or_default()
}

/// What the tab is called.
///
/// Excel refuses a name over thirty-one characters or holding any of
/// `[]:*?/\`, and refuses the whole file rather than the name - so this is
/// trimmed here rather than discovered by somebody whose export will not open.
fn tab_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|letter| !matches!(letter, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
        .take(31)
        .collect();

    if cleaned.trim().is_empty() {
        "Report".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonix_core::locale::Currency;
    use phonix_core::money::Money;
    use phonix_core::report::{PageSetup, RenderedBand, ReportTheme};

    fn report(bands: Vec<RenderedBand>) -> Rendered {
        Rendered {
            report_id: "test".to_owned(),
            title: "Product list".to_owned(),
            theme: ReportTheme::Modern,
            page: PageSetup::default(),
            bands,
        }
    }

    #[test]
    fn a_report_becomes_a_workbook() {
        let bytes = to_xlsx(&report(vec![RenderedBand::table(
            BandKind::Detail,
            vec!["Name".to_owned(), "Cost".to_owned()],
            vec![vec![
                Cell::text("Sofa"),
                Cell::money(Money::from_units(Currency::USD, 300).expect("an amount")),
            ]],
        )]))
        .expect("a workbook");

        // A zip, which is what an xlsx is.
        assert_eq!(bytes.get(..2), Some(b"PK".as_slice()));
    }

    #[test]
    fn a_tab_name_excel_refuses_is_trimmed_rather_than_kept() {
        assert_eq!(tab_name("Profit/loss: 2026"), "Profitloss 2026");
        assert_eq!(tab_name(""), "Report");
        assert_eq!(tab_name(&"x".repeat(40)).len(), 31);
    }

    #[test]
    fn an_amount_reaches_the_file_as_a_number() {
        let amount = Money::from_units(Currency::USD, 1_204).expect("an amount");

        assert!((as_number(&amount) - 1204.0).abs() < f64::EPSILON);
    }
}
