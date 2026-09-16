//! What a report is: its bands, the fields in them, and what an export costs.

use std::sync::Arc;

use phonix_core::money::Money;
use phonix_core::report::{
    Align, BandKind, ChartKind, ExportFormat, Logo, PageSetup, Point, Rendered, RenderedBand,
    ReportKind, ReportTheme,
};

use crate::l;
use crate::ui::table::Cell;

/// How one value is read out of a row.
type Read<T> = Arc<dyn Fn(&T) -> Cell + Send + Sync>;

/// Where a value goes, when it goes anywhere.
type Href<T> = Arc<dyn Fn(&T) -> Option<String> + Send + Sync>;

/// How a detail band reads its groups of lines.
type ReadLines<T> = Arc<dyn Fn(&T) -> Vec<RowGroup> + Send + Sync>;

/// How one line's share of a total is read.
type Amount<L> = Arc<dyn Fn(&L) -> Money + Send + Sync>;

/// How a chart band reads its points.
type ReadPoints<T> = Arc<dyn Fn(&T) -> Vec<Point> + Send + Sync>;

/// A value with its label in front of it, for a band drawn once.
///
/// A letterhead's fields carry their own labels, and a file has no second
/// column to put them in - `Closing balance 1,204.00` in one cell is what a
/// spreadsheet can show of a value that is labelled where it stands.
fn labelled<T: 'static>(field: &Field<T>, data: &T) -> Cell {
    let read = field.read(data);
    let value = read.to_text();

    match &field.label {
        // A label in front of the value, which is what a letterhead's
        // fields are: `Closing balance 1,204.00` in one cell. The type is
        // spent to say so, and a figure that stands alone keeps it.
        Some(label) if !value.is_empty() => Cell::text(format!("{label} {value}")),
        _ => read,
    }
}

/// One drawn value: what it says, and the record it opens.
///
/// The address is the screen's alone. Printing and every export write the
/// words: a printed page has nowhere to click, and a spreadsheet cell holding
/// an href is a cell nobody asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub cell: Cell,
    pub href: Option<String>,
}

/// Whether a group's rows fold away behind its header, and how the report
/// opens.
///
/// Folding is the browser's and nothing else's: it is not a setting, it does
/// not reach the server, and it does not survive a reload. A report opens in
/// the state its definition names every time, which is what stops two people
/// reading the same address and seeing different documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Folding {
    /// The header is a heading and nothing else. What a report gets unless it
    /// asks otherwise.
    #[default]
    Fixed,
    /// Folds, and opens open.
    Open,
    /// Folds, and opens closed - for a report of many groups where the
    /// subtotals are the answer and the rows are the evidence.
    Closed,
}

impl Folding {
    /// Whether the header is a control at all.
    pub const fn folds(self) -> bool {
        !matches!(self, Self::Fixed)
    }

    /// Whether a group is open when the report is drawn.
    pub const fn opens_open(self) -> bool {
        !matches!(self, Self::Closed)
    }
}

/// A run of lines drawn together, under a heading and over what they add up
/// to.
///
/// An ungrouped detail band is one group: no label, no totals, and the rows it
/// always had.
#[derive(Debug, Clone, PartialEq)]
pub struct RowGroup {
    /// What the group's header says. Empty draws no header.
    pub label: String,
    /// Whether the header folds the rows away, and how it opens.
    pub folding: Folding,
    pub rows: Vec<Vec<Value>>,
    /// One cell per column, empty where the column is not totalled. Empty
    /// altogether draws no footer.
    pub totals: Vec<Value>,
}

/// What a detail band groups by, and what each group adds up to.
///
/// ```ignore
/// Grouping::by(|item: &ItemSummary| item.category_name.clone())
///     .totalling("cost", |item: &ItemSummary| item.cost)
/// ```
pub struct Grouping<L: 'static> {
    label: Arc<dyn Fn(&L) -> String + Send + Sync>,
    totals: Vec<(&'static str, Amount<L>)>,
    folding: Folding,
}

impl<L: 'static> Grouping<L> {
    /// Group the lines by what this reads off them.
    pub fn by(label: impl Fn(&L) -> String + Send + Sync + 'static) -> Self {
        Self {
            label: Arc::new(label),
            totals: Vec::new(),
            folding: Folding::Fixed,
        }
    }

    /// Let a reader fold the rows away behind the header, and say how the
    /// report opens.
    #[must_use]
    pub const fn folding(mut self, folding: Folding) -> Self {
        self.folding = folding;
        self
    }

    /// Total this column, from the lines of the group rather than from what is
    /// drawn in it.
    #[must_use]
    pub fn totalling(
        mut self,
        key: &'static str,
        amount: impl Fn(&L) -> Money + Send + Sync + 'static,
    ) -> Self {
        self.totals.push((key, Arc::new(amount)));
        self
    }

    /// One group's totals, a cell per column.
    ///
    /// A group whose lines are in more than one currency totals nothing: a
    /// figure added across currencies is wrong in a way a reader cannot see.
    fn totals_of(&self, lines: &[L], keys: &[&'static str]) -> Vec<Value> {
        keys.iter()
            .map(|key| {
                let cell = self
                    .totals
                    .iter()
                    .find(|(totalled, _)| totalled == key)
                    .map_or(Cell::Empty, |(_, amount)| sum(amount, lines));

                Value { cell, href: None }
            })
            .collect()
    }
}

/// A band's headings, taken from the fields under them so the two cannot fall
/// out of step.
fn headings_of<L: 'static>(fields: &[Field<L>]) -> Vec<Heading> {
    fields
        .iter()
        .map(|field| Heading {
            key: field.key,
            label: field.label.clone(),
            align: field.align,
            figures: field.figures,
        })
        .collect()
}

/// One row of values per line.
fn rows_of<L: 'static>(fields: &[Field<L>], lines: &[L]) -> Vec<Vec<Value>> {
    lines
        .iter()
        .map(|line| fields.iter().map(|field| field.value(line)).collect())
        .collect()
}

/// The lines under their labels, in the order the labels first appear.
fn gathered<L: 'static>(grouping: &Grouping<L>, lines: Vec<L>) -> Vec<(String, Vec<L>)> {
    let mut groups: Vec<(String, Vec<L>)> = Vec::new();

    for line in lines {
        let label = (grouping.label)(&line);

        match groups.iter_mut().find(|(seen, _)| *seen == label) {
            Some((_, group)) => group.push(line),
            None => groups.push((label, vec![line])),
        }
    }

    groups
}

/// A detail band's groups, as the bands a writer sees.
///
/// Each group is its own header, its rows and its subtotal, in the order they
/// are drawn - the headings only on the first, because a file repeating them
/// between groups is a file a spreadsheet reads as several tables.
fn written(headings: Vec<String>, aligns: &[Align], groups: Vec<RowGroup>) -> Vec<RenderedBand> {
    let mut bands = Vec::new();

    for (index, group) in groups.into_iter().enumerate() {
        if !group.label.is_empty() {
            bands.push(RenderedBand::once(
                BandKind::GroupHeader,
                vec![Cell::text(group.label)],
            ));
        }

        bands.push(
            RenderedBand::table(
                BandKind::Detail,
                if index == 0 {
                    headings.clone()
                } else {
                    Vec::new()
                },
                group
                    .rows
                    .into_iter()
                    .map(|row| row.into_iter().map(|value| value.cell).collect())
                    .collect(),
            )
            .aligned(aligns.to_vec()),
        );

        if !group.totals.is_empty() {
            bands.push(
                RenderedBand::once(
                    BandKind::GroupFooter,
                    group.totals.into_iter().map(|value| value.cell).collect(),
                )
                .aligned(aligns.to_vec()),
            );
        }
    }

    bands
}

/// The subtotal row with its caption in the first column that has no figure of
/// its own. A row whose first column is totalled carries no caption: the
/// figure is what the column is for.
fn captioned(caption: &str, totals: Vec<Value>) -> Vec<Value> {
    let mut totals = totals;

    if let Some(free) = totals
        .iter_mut()
        .find(|value| matches!(value.cell, Cell::Empty))
    {
        *free = Value {
            cell: Cell::text(caption),
            href: None,
        };
    }

    totals
}

/// What a column of lines adds up to.
fn sum<L>(amount: &Amount<L>, lines: &[L]) -> Cell {
    let Some(first) = lines.first() else {
        return Cell::Empty;
    };

    Money::total(
        amount(first).currency(),
        lines.iter().map(|line| amount(line)),
    )
    .map_or(Cell::Empty, Cell::money)
}

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
    /// Where the value goes. `None` for most fields, and `Some` returning
    /// `None` for a row whose kind has no screen - a link to a page that is
    /// not there is worse than no link.
    pub(crate) href: Option<Href<T>>,
    pub(crate) align: Align,
    /// Whether the value is a figure, and so is set in numerals that line up
    /// under each other.
    pub(crate) figures: bool,
}

impl<T: 'static> Clone for Field<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            label: self.label.clone(),
            read: Arc::clone(&self.read),
            href: self.href.as_ref().map(Arc::clone),
            align: self.align,
            figures: self.figures,
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
            href: None,
            align: Align::Start,
            figures: false,
        }
    }

    /// A value with no label of its own.
    pub fn bare(key: &'static str, read: impl Fn(&T) -> Cell + Send + Sync + 'static) -> Self {
        Self {
            key,
            label: None,
            read: Arc::new(read),
            href: None,
            align: Align::Start,
            figures: false,
        }
    }

    /// A fixed piece of text, which is a field that does not read the row.
    pub fn text(key: &'static str, value: impl Into<String>) -> Self {
        let value: String = value.into();

        Self::bare(key, move |_| Cell::text(value.clone()))
    }

    /// A figure: money, a count, a quantity.
    ///
    /// It is set in numerals that line up under each other and it sits against
    /// the end of its box, so a definition stops writing `Align::End` beside
    /// every amount. A figure that belongs somewhere else - the ageing ladder
    /// down the left of a footer - says so with [`align`](Self::align) after.
    pub fn figure(
        key: &'static str,
        label: impl Into<String>,
        read: impl Fn(&T) -> Cell + Send + Sync + 'static,
    ) -> Self {
        Self {
            figures: true,
            align: Align::End,
            ..Self::new(key, label, read)
        }
    }

    /// A value that opens the record it names.
    ///
    /// ```ignore
    /// Field::link("number", l!("reports.column.document"), number, |line: &StatementLine| {
    ///     line.screen()
    /// })
    /// ```
    ///
    /// `href` returning `None` draws the value as ordinary text, which is what
    /// a document kind with no screen of its own gets.
    #[must_use]
    pub fn link(mut self, href: impl Fn(&T) -> Option<String> + Send + Sync + 'static) -> Self {
        self.href = Some(Arc::new(href));
        self
    }

    /// Which edge of its box the value sits against.
    #[must_use]
    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Read this field out of a row.
    pub fn read(&self, row: &T) -> Cell {
        (self.read)(row)
    }

    /// Read it with the address it opens, where it has one.
    pub fn value(&self, row: &T) -> Value {
        Value {
            cell: self.read(row),
            href: self.href.as_ref().and_then(|href| href(row)),
        }
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
    pub figures: bool,
}

/// How tall a chart is drawn when the definition does not say.
///
/// A third of a page: enough that a column is a shape rather than a tick, and
/// little enough that the rows it summarises are still on the sheet with it.
const DEFAULT_CHART_MM: f32 = 60.0;

/// A chart, as the report declares it.
///
/// The points are read off the report's own data - the same value the bands
/// read - so a chart cannot show something the table under it does not. It is
/// drawn as inline SVG on the server and printed as that SVG, which is what
/// makes the picture in the file the picture on the screen.
pub struct ChartBand<T: 'static> {
    pub(crate) kind: ChartKind,
    /// What the legend calls each series. One name and there is no legend: the
    /// band's own heading says what it is.
    pub(crate) series: Vec<String>,
    pub(crate) read: ReadPoints<T>,
    /// How tall the chart is drawn, in millimetres.
    pub(crate) height_mm: f32,
}

impl<T: 'static> Clone for ChartBand<T> {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            series: self.series.clone(),
            read: Arc::clone(&self.read),
            height_mm: self.height_mm,
        }
    }
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
    /// A picture of the same numbers.
    Chart(ChartBand<T>),
}

impl<T: 'static> Clone for Content<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Once(fields) => Self::Once(fields.clone()),
            Self::Lines { headings, read } => Self::Lines {
                headings: headings.clone(),
                read: Arc::clone(read),
            },
            Self::Chart(chart) => Self::Chart(chart.clone()),
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
        debug_assert!(
            !kind.is_group(),
            "a group band is declared with `Grouping` on the detail band, not on its own",
        );

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
        Self {
            kind: BandKind::Detail,
            content: Content::Lines {
                headings: headings_of(&fields),
                read: Arc::new(move |data| {
                    vec![RowGroup {
                        label: String::new(),
                        folding: Folding::Fixed,
                        rows: rows_of(&fields, &read(data)),
                        totals: Vec::new(),
                    }]
                }),
            },
        }
    }

    /// The detail band, in groups.
    ///
    /// ```ignore
    /// Band::grouped(
    ///     |page: &Page<ItemSummary>| page.rows.clone(),
    ///     fields,
    ///     Grouping::by(|item: &ItemSummary| item.category_name.clone())
    ///         .totalling("cost", |item: &ItemSummary| item.cost),
    /// )
    /// ```
    ///
    /// A group is where its lines are, not where they were sorted to: lines
    /// are gathered by label in the order the labels first appear, so a read
    /// that interleaves two categories still draws two groups.
    pub fn grouped<L: 'static>(
        read: impl Fn(&T) -> Vec<L> + Send + Sync + 'static,
        fields: Vec<Field<L>>,
        grouping: Grouping<L>,
    ) -> Self {
        let keys: Vec<&'static str> = fields.iter().map(|field| field.key).collect();
        let caption = l!("reports.subtotal");

        Self {
            kind: BandKind::Detail,
            content: Content::Lines {
                headings: headings_of(&fields),
                read: Arc::new(move |data| {
                    gathered(&grouping, read(data))
                        .into_iter()
                        .map(|(label, lines)| RowGroup {
                            label,
                            folding: grouping.folding,
                            rows: rows_of(&fields, &lines),
                            totals: captioned(&caption, grouping.totals_of(&lines, &keys)),
                        })
                        .collect()
                }),
            },
        }
    }

    /// A chart band, over points read from the report's own data.
    ///
    /// ```ignore
    /// Band::chart(
    ///     BandKind::ReportFooter,
    ///     ChartKind::Column,
    ///     vec![l!("items.cost")],
    ///     |page: &Page<ItemSummary>| by_category(page),
    /// )
    /// ```
    ///
    /// The band kind is the chart's place in the report rather than a kind of
    /// its own: a chart over the rows goes in the report footer, one above
    /// them in the header. A report whose only band is a chart is a report.
    pub fn chart(
        kind: BandKind,
        chart: ChartKind,
        series: Vec<String>,
        read: impl Fn(&T) -> Vec<Point> + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            content: Content::Chart(ChartBand {
                kind: chart,
                series,
                read: Arc::new(read),
                height_mm: DEFAULT_CHART_MM,
            }),
        }
    }

    /// How tall the chart is drawn. Only read by a chart band.
    #[must_use]
    pub fn height_mm(mut self, height_mm: f32) -> Self {
        if let Content::Chart(chart) = &mut self.content {
            chart.height_mm = height_mm;
        }

        self
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
            Content::Chart(_) => debug_assert!(
                false,
                "`{}` was added to a chart band, which draws its points and nothing else",
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
    /// What somebody must hold to read it. The viewer refuses without it, and
    /// the index does not list it.
    pub(crate) permission: &'static str,
    pub(crate) title: String,
    pub(crate) kind: ReportKind,
    /// Which of the workspace's documents this draws, if it draws one of them.
    /// `None` for a list: a product list is not something a tenant keeps
    /// document settings about.
    pub(crate) document_type: Option<&'static str>,
    pub(crate) theme: ReportTheme,
    pub(crate) page: PageSetup,
    /// `None` draws no logo. A document setting can put one back.
    pub(crate) logo: Option<Logo>,
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
            permission: self.permission,
            title: self.title.clone(),
            kind: self.kind,
            document_type: self.document_type,
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
    ///
    /// `permission` is an argument rather than a builder step because a report
    /// that forgot one would be readable by anybody who knows the address.
    pub fn new(
        id: &'static str,
        permission: &'static str,
        title: impl Into<String>,
        kind: ReportKind,
    ) -> Self {
        Self {
            id,
            permission,
            title: title.into(),
            kind,
            document_type: None,
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

    /// The workspace document this draws, by the name `config/numbering/`
    /// gives it.
    ///
    /// Saying so is what lets the tenant's document settings override the look,
    /// the paper and the mark this definition chose - see
    /// [`DocumentStyles`](super::DocumentStyles).
    #[must_use]
    pub const fn document_type(mut self, document_type: &'static str) -> Self {
        self.document_type = Some(document_type);
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

    /// Draw the workspace logo, here and this tall.
    ///
    /// A workspace with no logo set draws its name in the same place rather
    /// than leaving a hole in the letterhead.
    #[must_use]
    pub const fn logo(mut self, logo: Logo) -> Self {
        self.logo = Some(logo);
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

    /// The report with its types erased, ready to be written out.
    ///
    /// What crosses to `phonix-services`, which can see neither this type nor
    /// the row type it is closed over. The screen does not use it - it draws
    /// from the definition directly - so this is the export path's view of the
    /// same report, produced by the same closures.
    ///
    /// The look and the page are the definition's own. A document's settings
    /// reach the screen through `DocumentStyles`, which is a browser context;
    /// the writer that needs them is the PDF one, and resolving them here is
    /// that item's to do.
    pub fn rendered(&self, data: &T) -> Rendered {
        let bands = self
            .bands
            .iter()
            .flat_map(|band| match &band.content {
                Content::Once(fields) => vec![
                    RenderedBand::once(
                        band.kind,
                        fields.iter().map(|field| labelled(field, data)).collect(),
                    )
                    .aligned(fields.iter().map(|field| field.align).collect()),
                ],
                // Deliberately nothing. A chart written out as numbers would
                // be a column of figures in a spreadsheet that nobody asked
                // for, under a heading that says it is a picture - and the
                // rows it was drawn from are already in the file.
                Content::Chart(_) => Vec::new(),
                Content::Lines { headings, read } => {
                    let aligns: Vec<Align> = headings.iter().map(|heading| heading.align).collect();
                    let labels = headings
                        .iter()
                        .map(|heading| heading.label.clone().unwrap_or_default())
                        .collect();

                    written(labels, &aligns, read(data))
                }
            })
            .collect();

        Rendered {
            report_id: self.id.to_owned(),
            title: self.title.clone(),
            theme: self.theme,
            page: self.page,
            bands,
        }
    }

    /// The band of this kind, if the report declares one.
    pub fn band_of(&self, kind: BandKind) -> Option<&Band<T>> {
        self.bands.iter().find(|band| band.kind == kind)
    }

    /// Which of the workspace's documents this draws, if it draws one. What
    /// the export path dresses a report in the workspace's own settings by.
    pub const fn document(&self) -> Option<&'static str> {
        self.document_type
    }

    /// What somebody must hold to read this report.
    pub const fn permission(&self) -> &'static str {
        self.permission
    }

    /// Whether an export of this report is a job or an answer, and what
    /// bounds it when it is an answer.
    pub const fn extent(&self) -> &Extent {
        &self.extent
    }
}

#[cfg(test)]
mod tests {
    use phonix_core::locale::Currency;
    use phonix_core::permissions;

    use super::*;

    #[derive(Clone)]
    struct Line {
        name: &'static str,
        kind: &'static str,
        cost: Money,
    }

    impl Line {
        fn new(name: &'static str, kind: &'static str, units: i64) -> Self {
            Self {
                name,
                kind,
                cost: Money::from_units(Currency::USD, units).expect("a small amount"),
            }
        }
    }

    struct Statement {
        lines: Vec<Line>,
    }

    fn definition() -> ReportDefinition<Statement> {
        ReportDefinition::new("test", permissions::REPORTS, "Test", ReportKind::List)
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
            lines: vec![
                Line::new("Sofa", "seating", 300),
                Line::new("Lamp", "lighting", 40),
            ],
        };

        let Some(Content::Lines { headings, read }) =
            report.band_of(BandKind::Detail).map(|band| &band.content)
        else {
            panic!("a detail band over lines");
        };

        assert_eq!(headings.len(), 1);
        assert_eq!(
            read(&statement)
                .into_iter()
                .flat_map(|group| group.rows)
                .flatten()
                .map(|value| value.cell)
                .collect::<Vec<_>>(),
            vec![Cell::text("Sofa"), Cell::text("Lamp")]
        );
    }

    #[test]
    fn a_group_totals_its_own_lines() {
        let report = ReportDefinition::new("test", permissions::REPORTS, "Test", ReportKind::List)
            .band(Band::grouped(
                |statement: &Statement| statement.lines.clone(),
                vec![Field::new("name", "Name", |line: &Line| {
                    Cell::text(line.name)
                })],
                Grouping::by(|line: &Line| line.kind.to_owned())
                    .totalling("name", |line: &Line| line.cost),
            ));

        let statement = Statement {
            lines: vec![
                Line::new("sofa", "seating", 300),
                Line::new("stool", "seating", 150),
                Line::new("lamp", "lighting", 40),
            ],
        };

        let Some(Content::Lines { read, .. }) =
            report.band_of(BandKind::Detail).map(|band| &band.content)
        else {
            panic!("a detail band over lines");
        };

        let groups = read(&statement);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "seating");
        assert_eq!(groups[0].rows.len(), 2);
        assert_eq!(
            groups[0].totals[0].cell,
            Cell::text(
                Money::from_units(Currency::USD, 450)
                    .expect("450")
                    .to_display_string()
            )
        );
        assert_eq!(groups[1].label, "lighting");
        assert_eq!(groups[1].rows.len(), 1);
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
