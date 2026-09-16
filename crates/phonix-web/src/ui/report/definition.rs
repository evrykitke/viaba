//! What a report is: its bands, the fields in them, and what an export costs.

use std::sync::Arc;

use phonix_core::report::{
    Align, BandKind, ExportFormat, LogoPlacement, PageSetup, ReportKind, ReportTheme,
};

use crate::ui::table::Cell;

/// How one value is read out of a row.
type Read<T> = Arc<dyn Fn(&T) -> Cell + Send + Sync>;

/// How a detail band reads a row of cells per line.
type ReadLines<T> = Arc<dyn Fn(&T) -> Vec<Vec<Cell>> + Send + Sync>;

/// One value in a band: a field of the row, or a constant beside it.
///
/// The closure is what makes a definition Rust rather than a data file - a
/// field that names nothing does not compile.
pub struct Field<T: 'static> {
    /// A stable identifier. It keys the column in an export, and it is not the
    /// heading: renaming the heading is a wording change.
    pub(crate) key: &'static str,
    /// What it is called on the page. `None` for a value that stands alone -
    /// a title, a total whose row already says what it is.
    pub(crate) label: Option<String>,
    pub(crate) read: Read<T>,
    pub(crate) align: Align,
}

impl<T: 'static> Clone for Field<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            label: self.label.clone(),
            read: Arc::clone(&self.read),
            align: self.align,
        }
    }
}

impl<T: 'static> Field<T> {
    /// A labelled value read out of the row.
    ///
    /// ```ignore
    /// Field::new("closing_balance", l!("statement.closing"), |s: &CustomerStatement| {
    ///     Cell::text(s.closing.to_string())
    /// })
    /// .align(Align::End)
    /// ```
    pub fn new(
        key: &'static str,
        label: impl Into<String>,
        read: impl Fn(&T) -> Cell + Send + Sync + 'static,
    ) -> Self {
        Self {
            key,
            label: Some(label.into()),
            read: Arc::new(read),
            align: Align::Start,
        }
    }

    /// A value with no label of its own.
    pub fn bare(key: &'static str, read: impl Fn(&T) -> Cell + Send + Sync + 'static) -> Self {
        Self {
            key,
            label: None,
            read: Arc::new(read),
            align: Align::Start,
        }
    }

    /// A fixed piece of text, which is a field that does not read the row.
    pub fn text(key: &'static str, value: impl Into<String>) -> Self {
        let value: String = value.into();

        Self::bare(key, move |_| Cell::text(value.clone()))
    }

    /// Which edge of its box the value sits against. Money and counts are
    /// [`Align::End`].
    #[must_use]
    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Read this field out of a row.
    pub fn read(&self, row: &T) -> Cell {
        (self.read)(row)
    }
}

/// One column of a detail band: what it is called, and which edge it sits
/// against.
#[derive(Debug, Clone)]
pub struct Heading {
    /// The stable identifier, which is what an export names the column by.
    pub key: &'static str,
    pub label: Option<String>,
    pub align: Align,
}

/// What a band draws.
pub(crate) enum Content<T: 'static> {
    /// Values read once out of the report's data: a letterhead, a total.
    Once(Vec<Field<T>>),
    /// One row per line of a sequence the data holds. The cells are read
    /// through `Field<L>` closures over the line type and erased here, so the
    /// headings and the cells under them cannot fall out of step.
    Lines {
        headings: Vec<Heading>,
        read: ReadLines<T>,
    },
}

impl<T: 'static> Clone for Content<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Once(fields) => Self::Once(fields.clone()),
            Self::Lines { headings, read } => Self::Lines {
                headings: headings.clone(),
                read: Arc::clone(read),
            },
        }
    }
}

/// One band of a report, and what is drawn in it.
pub struct Band<T: 'static> {
    pub(crate) kind: BandKind,
    pub(crate) content: Content<T>,
}

impl<T: 'static> Clone for Band<T> {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            content: self.content.clone(),
        }
    }
}

impl<T: 'static> Band<T> {
    /// A band drawn once, from the report's own data.
    pub const fn new(kind: BandKind) -> Self {
        Self {
            kind,
            content: Content::Once(Vec::new()),
        }
    }

    /// The detail band, over a sequence the report's data holds.
    ///
    /// ```ignore
    /// Band::lines(
    ///     |statement: &CustomerStatement| statement.lines.clone(),
    ///     vec![Field::new("number", l!("reports.column.document"), |line: &StatementLine| {
    ///         Cell::text(line.number.clone())
    ///     })],
    /// )
    /// ```
    pub fn lines<L: 'static>(
        read: impl Fn(&T) -> Vec<L> + Send + Sync + 'static,
        fields: Vec<Field<L>>,
    ) -> Self {
        let headings = fields
            .iter()
            .map(|field| Heading {
                key: field.key,
                label: field.label.clone(),
                align: field.align,
            })
            .collect();

        Self {
            kind: BandKind::Detail,
            content: Content::Lines {
                headings,
                read: Arc::new(move |data| {
                    read(data)
                        .iter()
                        .map(|line| fields.iter().map(|field| field.read(line)).collect())
                        .collect()
                }),
            },
        }
    }

    /// Add a field. Order here is order across the band.
    #[must_use]
    pub fn field(mut self, field: Field<T>) -> Self {
        match &mut self.content {
            Content::Once(fields) => fields.push(field),
            Content::Lines { .. } => debug_assert!(
                false,
                "`{}` was added to a detail band, whose columns are its lines' fields",
                field.key,
            ),
        }

        self
    }
}

/// How much work an export of this report is, which decides how it is run.
///
/// Growing with the data means a job: a statement over a year of a busy ledger
/// is minutes of rendering, and a request holding a connection open for all of
/// it loses the work when somebody closes the tab. Bounded by the record it is
/// about means the bytes come back from the click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extent {
    /// A known amount of work, and what bounds it - one line, the way an
    /// unpaged query carries one.
    Bounded(&'static str),
    /// As much work as there is data.
    Grows,
}

/// Everything one report needs to say about itself.
///
/// Cheap to clone: every closure inside is behind an `Arc`, so a screen can
/// build one per render.
pub struct ReportDefinition<T: 'static> {
    /// A stable name. It keys the ids that tie the toolbar to the report, and
    /// it is the stem an exported file is named with.
    pub(crate) id: &'static str,
    pub(crate) title: String,
    pub(crate) kind: ReportKind,
    pub(crate) theme: ReportTheme,
    pub(crate) page: PageSetup,
    /// `None` draws no logo. A document setting can put one back.
    pub(crate) logo: Option<LogoPlacement>,
    pub(crate) bands: Vec<Band<T>>,
    pub(crate) extent: Extent,
    /// The formats the export menu offers. Empty draws no menu at all, which
    /// is what a report whose writers have not landed yet should show.
    pub(crate) formats: Vec<ExportFormat>,
}

impl<T: 'static> Clone for ReportDefinition<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            title: self.title.clone(),
            kind: self.kind,
            theme: self.theme,
            page: self.page,
            logo: self.logo,
            bands: self.bands.clone(),
            extent: self.extent.clone(),
            formats: self.formats.clone(),
        }
    }
}

impl<T: 'static> ReportDefinition<T> {
    /// A report with no bands yet, in the default look on a default page.
    ///
    /// It grows with its data until it says otherwise: a report wrongly called
    /// bounded holds a connection open for however long it takes, and a job
    /// that did not need to be one is only slower.
    pub fn new(id: &'static str, title: impl Into<String>, kind: ReportKind) -> Self {
        Self {
            id,
            title: title.into(),
            kind,
            theme: ReportTheme::default(),
            page: PageSetup::default(),
            logo: None,
            bands: Vec::new(),
            extent: Extent::Grows,
            formats: Vec::new(),
        }
    }

    /// Add a band. Order here is the order they are declared, not the order
    /// they are drawn - that is [`BandKind`]'s.
    #[must_use]
    pub fn band(mut self, band: Band<T>) -> Self {
        debug_assert!(
            !self.bands.iter().any(|existing| existing.kind == band.kind),
            "`{}` declares {:?} twice, and a report draws each band once",
            self.id,
            band.kind,
        );

        self.bands.push(band);
        self
    }

    /// The look this report asks for, and the margins that come with it. A
    /// document setting overrides both, and so does a later [`page`](Self::page).
    #[must_use]
    pub const fn theme(mut self, theme: ReportTheme) -> Self {
        self.theme = theme;
        self.page.margins = theme.metrics().margins;
        self
    }

    /// The page it is laid out on. A document setting overrides it.
    #[must_use]
    pub const fn page(mut self, page: PageSetup) -> Self {
        self.page = page;
        self
    }

    /// Draw the workspace logo, here.
    #[must_use]
    pub const fn logo(mut self, placement: LogoPlacement) -> Self {
        self.logo = Some(placement);
        self
    }

    /// Offer this format in the export menu.
    #[must_use]
    pub fn exports(mut self, format: ExportFormat) -> Self {
        if !self.formats.contains(&format) {
            self.formats.push(format);
        }

        self
    }

    /// Declare the report bounded, and say by what.
    ///
    /// ```ignore
    /// .bounded_by("one payment and its allocations")
    /// ```
    ///
    /// The sentence is the point: it is the same claim an unpaged query makes
    /// on its doc comment, and the next person to add a band to this report
    /// has to decide whether it is still true.
    #[must_use]
    pub const fn bounded_by(mut self, reason: &'static str) -> Self {
        debug_assert!(
            !reason.is_empty(),
            "a report that says it is bounded has to say by what",
        );

        self.extent = Extent::Bounded(reason);
        self
    }

    /// The band of this kind, if the report declares one.
    pub fn band_of(&self, kind: BandKind) -> Option<&Band<T>> {
        self.bands.iter().find(|band| band.kind == kind)
    }

    /// Whether an export of this report is a job or an answer, and what
    /// bounds it when it is an answer.
    pub const fn extent(&self) -> &Extent {
        &self.extent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Line {
        name: &'static str,
    }

    struct Statement {
        lines: Vec<Line>,
    }

    fn definition() -> ReportDefinition<Statement> {
        ReportDefinition::new("test", "Test", ReportKind::List)
            .band(Band::new(BandKind::ReportHeader).field(Field::text("title", "Test")))
            .band(Band::lines(
                |statement: &Statement| statement.lines.clone(),
                vec![Field::new("name", "Name", |line: &Line| {
                    Cell::text(line.name)
                })],
            ))
    }

    #[test]
    fn a_detail_band_reads_one_row_per_line() {
        let report = definition();
        let statement = Statement {
            lines: vec![Line { name: "Sofa" }, Line { name: "Lamp" }],
        };

        let Some(Content::Lines { headings, read }) =
            report.band_of(BandKind::Detail).map(|band| &band.content)
        else {
            panic!("a detail band over lines");
        };

        assert_eq!(headings.len(), 1);
        assert_eq!(
            read(&statement),
            vec![vec![Cell::text("Sofa")], vec![Cell::text("Lamp")]]
        );
    }

    #[test]
    fn a_report_grows_until_it_says_otherwise() {
        assert_eq!(definition().extent(), &Extent::Grows);
        assert_eq!(
            definition().bounded_by("one row").extent(),
            &Extent::Bounded("one row")
        );
    }
}
