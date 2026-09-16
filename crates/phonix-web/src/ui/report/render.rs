//! One component that draws a report from its definition.
//!
//! The three looks are one set of bands measured differently: the theme's
//! metrics reach the markup as millimetres and points in a `style`, and the
//! classes say only which colour a rule or a heading is. A sheet is drawn at
//! the width it will print at.
//!
//! Nothing here reads the clock, for the reason
//! [`pages::sales::reports`](crate::pages::sales::reports) gives: a value
//! worked out during the render and again during hydration is a mismatch, and
//! a mismatch costs every handler on the page.
//!
//! A report is drawn from one value. Its bands read that value directly; the
//! detail band reads the sequence inside it, which is what lets a letterhead
//! show a customer and a total while the rows below are that customer's lines.
//! A group band is not declared: the detail band's [`Grouping`] says what the
//! groups are, and a header and a subtotal are drawn around each of them.
//!
//! [`Grouping`]: super::Grouping

use std::sync::OnceLock;

use leptos::prelude::*;
use phonix_core::report::{
    Align, BandKind, ChartKind, Colour, DocumentSettings, Logo, Mark, Metrics, Plot, Typeface, plot,
};

use leptos_router::components::A;

use super::definition::{ChartBand, Content};
use super::{
    Band, DocumentStyles, Field, Heading, Letterhead, Paging, ReportDefinition, RowGroup, Value,
};

/// Draw a report.
///
/// ```ignore
/// <Report definition=customer_statement() data=statement />
/// ```
#[component]
pub fn report<T>(definition: ReportDefinition<T>, data: T) -> impl IntoView
where
    T: Send + Sync + 'static,
{
    // Stored so the body can be a closure: the settings arrive after the first
    // paint, and a report that drew once would keep the look its definition
    // named however the workspace had answered.
    let data = StoredValue::new(data);
    let definition = StoredValue::new(definition);

    move || {
        definition.with_value(|definition| data.with_value(|data| report_view(definition, data)))
    }
}

/// One report, drawn in the settings that apply to it now.
fn report_view<T>(definition: &ReportDefinition<T>, data: &T) -> AnyView
where
    T: Send + Sync + 'static,
{
    // The definition's own choice is the default and the workspace's setting
    // overrides it - for the mark as much as for the look, so a document
    // settings row with no mark draws none.
    let style = definition.document_type.and_then(DocumentStyles::of);
    let theme = style.as_ref().map_or(definition.theme, |kept| kept.theme);
    let page = style
        .as_ref()
        .map_or(definition.page, DocumentSettings::page);
    let logo = style.as_ref().map_or(definition.logo, |kept| kept.logo);
    let header_text = style.as_ref().and_then(|kept| kept.header_text.clone());
    let footer_text = style.and_then(|kept| kept.footer_text);

    let metrics = theme.metrics();

    let sheet = format!(
        "width:{}mm;padding:{}mm {}mm {}mm {}mm;font-size:{}pt",
        page.width_mm(),
        page.margins.top,
        page.margins.right,
        page.margins.bottom,
        page.margins.left,
        metrics.type_scale.body_pt,
    );

    // Where the mark goes decides what sits beside it: against an edge the
    // letterhead reads across the page, and centred it reads down it.
    let letterhead_mark = logo.filter(|logo| logo.placement.band() == BandKind::ReportHeader);

    let letterhead = definition.band_of(BandKind::ReportHeader).map(|band| {
        let body = view! {
            <div class="min-w-0 flex-1">
                <h1
                    class=heading_ink(metrics.colour)
                    style=format!("font-size:{}pt", metrics.type_scale.title_pt)
                >
                    {definition.title.clone()}
                </h1>
                {band_content(band, data, &metrics)}
                {words(header_text.clone(), &metrics)}
            </div>
        };

        view! {
            <header
                class=format!("block {}", rule_ink(metrics.colour))
                style=format!(
                    "min-height:{}mm;border-bottom:{}mm solid",
                    metrics.bands.report_header,
                    metrics.rules.under_letterhead,
                )
            >
                {letterhead_layout(letterhead_mark, body.into_any(), &metrics)}
            </header>
        }
    });

    let page_header = definition.band_of(BandKind::PageHeader).map(|band| {
        let band = running_band(band, data, &metrics, BandKind::PageHeader);

        view! {
            {logo
                .filter(|logo| logo.placement.band() == BandKind::PageHeader)
                .map(|logo| mark_row(logo, &metrics))}
            {band}
        }
    });
    let page_footer = definition
        .band_of(BandKind::PageFooter)
        .map(|band| running_band(band, data, &metrics, BandKind::PageFooter));
    let footer = definition.band_of(BandKind::ReportFooter).map(|band| {
        view! {
            <footer
                class=format!(
                    "block {} {}",
                    rule_ink(metrics.colour),
                    weight_ink(metrics.colour),
                )
                style=format!(
                    "min-height:{}mm;border-top:{}mm solid;font-size:{}pt",
                    metrics.bands.report_footer,
                    metrics.rules.above_total,
                    metrics.type_scale.total_pt,
                )
            >
                {band_content(band, data, &metrics)}
                {words(footer_text.clone(), &metrics)}
            </footer>
        }
    });

    // The rows this page holds, which is every row where nothing is paging the
    // report - a document, or a preview drawn outside the viewer.
    let detail =
        definition
            .band_of(BandKind::Detail)
            .map(|band| match (&band.content, Paging::get()) {
                (Content::Lines { headings, read }, Some(paging)) => {
                    lines(headings, &paged(&read(data), &paging.window()), &metrics)
                }
                _ => band_content(band, data, &metrics),
            });

    view! {
        // The one element printing keeps. See `viewer`.
        <article
            data-report-sheet=""
            class="mx-auto bg-surface text-content shadow-card"
            style=sheet
        >
            {letterhead}
            {page_header}
            {detail}
            {footer}
            {page_footer}
        </article>
    }
    .into_any()
}

/// How wide a chart's own coordinates are. The sheet scales it, so this is an
/// aspect and not a size.
const CHART_WIDTH: f32 = 160.0;

/// How many colours a chart has before a series has to do without one.
///
/// Six, assigned in a fixed order and never cycled: a seventh series drawn in
/// the first one's colour is two things the reader believes are one.
const SERIES_COLOURS: usize = 6;

/// A chart band, as the SVG the screen draws and the browser prints.
fn drawn_chart<T>(chart: &ChartBand<T>, data: &T, metrics: &Metrics) -> AnyView
where
    T: Send + Sync + 'static,
{
    let points = (chart.read)(data);
    let height = CHART_WIDTH * (chart.height_mm / 90.0);
    let drawn = plot(chart.kind, &points, CHART_WIDTH, height);

    // The chart's own units are not points, so the type inside it is sized
    // against the box: a label set in points inside a drawing that is then
    // scaled would not be the size it says.
    let ink = metrics.type_scale.caption_pt / 2.4;
    let series = chart.series.clone();

    view! {
        <figure
            class="w-full"
            style=format!(
                "padding:{}mm {}mm",
                metrics.padding.vertical,
                metrics.padding.horizontal,
            )
        >
            <svg
                viewBox=format!("0 0 {CHART_WIDTH} {height}")
                width="100%"
                style=format!("height:{}mm", chart.height_mm)
                role="img"
            >
                {grid(&drawn, ink)}
                {marks(&drawn)}
                {axis_labels(&drawn, ink)}
            </svg>

            {(series.len() > 1).then(|| legend(&series, metrics))}
        </figure>
    }
    .into_any()
}

/// The ticks, and the lines across from them.
///
/// Recessive on purpose: a grid is for reading a bar against, not for looking
/// at. `currentColor` at a low opacity, so it is the sheet's own ink in both
/// themes and on paper.
fn grid(drawn: &Plot, ink: f32) -> AnyView {
    if !drawn.kind.axes() {
        return ().into_any();
    }

    let frame = drawn.plot;
    let across = drawn.kind.horizontal();

    drawn
        .ticks
        .iter()
        .map(|tick| {
            let (x1, y1, x2, y2) = if across {
                (tick.at, frame.y, tick.at, frame.bottom())
            } else {
                (frame.x, tick.at, frame.right(), tick.at)
            };

            let (label_x, label_y, anchor) = if across {
                (tick.at, frame.bottom() + ink * 1.4, "middle")
            } else {
                (frame.x - ink * 0.6, tick.at + ink * 0.35, "end")
            };

            view! {
                <line
                    x1=x1
                    y1=y1
                    x2=x2
                    y2=y2
                    stroke="currentColor"
                    stroke-width="0.25"
                    opacity="0.18"
                />
                <text
                    x=label_x
                    y=label_y
                    text-anchor=anchor
                    font-size=ink
                    fill="currentColor"
                    opacity="0.6"
                >
                    {figure(tick.value)}
                </text>
            }
        })
        .collect_view()
        .into_any()
}

/// The marks themselves.
fn marks(drawn: &Plot) -> AnyView {
    let frame = drawn.plot;
    let centre = (frame.x + frame.right()) / 2.0;
    let middle = (frame.y + frame.bottom()) / 2.0;
    let radius = frame.width.min(frame.height) / 2.0;
    let donut = drawn.kind == ChartKind::Donut;

    drawn
        .marks
        .iter()
        .map(|mark| match mark {
            Mark::Rect {
                x,
                y,
                width,
                height,
                series,
                ..
            } => {
                // A rounded end and a hairline of the surface between one bar
                // and the next, so touching marks stay two marks.
                let width = (width - 0.4).max(0.2);

                view! {
                    <rect x=*x y=*y width=width height=*height rx="0.6" fill=colour(*series) />
                }
                .into_any()
            }
            Mark::Path {
                points,
                series,
                filled,
            } => {
                let line = points
                    .iter()
                    .enumerate()
                    .map(|(index, (x, y))| {
                        format!("{} {x:.2} {y:.2}", if index == 0 { "M" } else { "L" })
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let under = points.last().zip(points.first()).map(|(last, first)| {
                    format!(
                        "{line} L {:.2} {:.2} L {:.2} {:.2} Z",
                        last.0,
                        frame.bottom(),
                        first.0,
                        frame.bottom(),
                    )
                });

                let filled = *filled;

                view! {
                    {under
                        .filter(|_| filled)
                        .map(|under| {
                            view! { <path d=under fill=colour(*series) opacity="0.18" /> }
                        })}
                    <path
                        d=line
                        fill="none"
                        stroke=colour(*series)
                        stroke-width="0.7"
                        stroke-linejoin="round"
                        stroke-linecap="round"
                    />
                }
                .into_any()
            }
            Mark::Slice {
                from, to, series, ..
            } => {
                let hole = if donut { radius * 0.55 } else { 0.0 };

                view! {
                    <path
                        d=segment(centre, middle, radius, hole, *from, *to)
                        fill=colour(*series)
                        stroke="var(--color-surface)"
                        stroke-width="0.4"
                    />
                }
                .into_any()
            }
        })
        .collect_view()
        .into_any()
}

/// What each point is called, beside the marks.
///
/// Every label where they fit and every second or third where they do not: a
/// chart of thirty categories wants fifteen labels rather than a grey smear.
fn axis_labels(drawn: &Plot, ink: f32) -> AnyView {
    if !drawn.kind.axes() || drawn.labels.is_empty() {
        return ().into_any();
    }

    let frame = drawn.plot;
    let across = drawn.kind.horizontal();
    let room = if across { frame.height } else { frame.width };
    let slot = room / drawn.labels.len() as f32;
    let every = ((ink * 3.5) / slot).ceil().max(1.0) as usize;

    drawn
        .labels
        .iter()
        .enumerate()
        .filter(|(index, _)| index % every == 0)
        .map(|(index, label)| {
            let along = slot * (index as f32 + 0.5);

            let (x, y, anchor) = if across {
                (frame.x - ink * 0.6, frame.y + along + ink * 0.35, "end")
            } else {
                (frame.x + along, frame.bottom() + ink * 1.4, "middle")
            };

            view! {
                <text
                    x=x
                    y=y
                    text-anchor=anchor
                    font-size=ink
                    fill="currentColor"
                    opacity="0.7"
                >
                    {label.clone()}
                </text>
            }
        })
        .collect_view()
        .into_any()
}

/// What each series is called, beside the colour it is drawn in.
///
/// Drawn wherever there is more than one series, because colour on its own is
/// not an identity anybody can read. One series needs none: the band it is in
/// already says what it is.
fn legend(series: &[String], metrics: &Metrics) -> AnyView {
    let size = metrics.type_scale.caption_pt;

    view! {
        <ul
            class="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-content-subtle"
            style=format!("font-size:{size}pt")
        >
            {series
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    view! {
                        <li class="flex items-center gap-1">
                            <span
                                class="inline-block size-2 shrink-0 rounded-[2px]"
                                style=format!("background:{}", colour(index))
                            ></span>
                            {name.clone()}
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
    .into_any()
}

/// The colour a series is drawn in.
///
/// A fixed order out of the chart palette, never cycled: a seventh series is
/// drawn in the ink the axis is, which reads as "another" rather than as one
/// of the six.
fn colour(series: usize) -> String {
    if series < SERIES_COLOURS {
        format!("var(--chart-{})", series + 1)
    } else {
        "currentColor".to_owned()
    }
}

/// One slice of a pie, or of a donut when there is a hole in it.
fn segment(cx: f32, cy: f32, radius: f32, hole: f32, from: f32, to: f32) -> String {
    let point = |angle: f32, radius: f32| {
        // Clockwise from twelve, which is where a reader starts.
        let radians = (angle - 90.0).to_radians();

        (cx + radius * radians.cos(), cy + radius * radians.sin())
    };

    let long = i32::from(to - from > 180.0);
    let (sx, sy) = point(from, radius);
    let (ex, ey) = point(to, radius);

    if hole <= 0.0 {
        return format!(
            "M {cx:.2} {cy:.2} L {sx:.2} {sy:.2} \
             A {radius:.2} {radius:.2} 0 {long} 1 {ex:.2} {ey:.2} Z",
        );
    }

    let (ix, iy) = point(to, hole);
    let (jx, jy) = point(from, hole);

    format!(
        "M {sx:.2} {sy:.2} A {radius:.2} {radius:.2} 0 {long} 1 {ex:.2} {ey:.2} \
         L {ix:.2} {iy:.2} A {hole:.2} {hole:.2} 0 {long} 0 {jx:.2} {jy:.2} Z",
    )
}

/// A tick's value, short enough to stand beside an axis.
fn figure(value: f64) -> String {
    let magnitude = value.abs();

    if magnitude >= 1_000_000.0 {
        format!("{:.1}m", value / 1_000_000.0)
    } else if magnitude >= 1_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else if magnitude >= 10.0 || value == value.trunc() {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// The tenant's own words at the head or the foot of a document.
///
/// Drawn as written: these are not an i18n key and are never looked up.
fn words(text: Option<String>, metrics: &Metrics) -> AnyView {
    let size = metrics.type_scale.caption_pt;
    let padding = metrics.padding;

    text.map(|text| {
        view! {
            <p
                class="whitespace-pre-line text-content-muted"
                style=format!(
                    "padding:0 {}mm {}mm;font-size:{size}pt",
                    padding.horizontal,
                    padding.vertical,
                )
            >
                {text}
            </p>
        }
    })
    .into_any()
}

/// A band with no rule of its own: the page bands.
fn running_band<T>(band: &Band<T>, data: &T, metrics: &Metrics, kind: BandKind) -> AnyView
where
    T: Send + Sync + 'static,
{
    let size = if kind.repeats_per_page() {
        metrics.type_scale.caption_pt
    } else {
        metrics.type_scale.heading_pt
    };

    view! {
        <div
            class=if kind.repeats_per_page() { "text-content-subtle" } else { "font-medium" }
            style=format!("min-height:{}mm;font-size:{}pt", metrics.bands.of(kind), size)
        >
            {band_content(band, data, metrics)}
        </div>
    }
    .into_any()
}

/// What is inside a band: values read once, or a row per line.
fn band_content<T>(band: &Band<T>, data: &T, metrics: &Metrics) -> AnyView
where
    T: Send + Sync + 'static,
{
    match &band.content {
        Content::Once(fields) => once(fields, data, metrics),
        Content::Lines { headings, read } => lines(headings, &read(data), metrics),
        Content::Chart(chart) => drawn_chart(chart, data, metrics),
    }
}

/// A band's fields, gathered against the three edges they align to.
fn once<T>(fields: &[Field<T>], data: &T, metrics: &Metrics) -> AnyView
where
    T: Send + Sync + 'static,
{
    let group = |align: Align| {
        fields
            .iter()
            .filter(|field| field.align == align)
            .map(|field| field_view(field, data, metrics))
            .collect_view()
    };

    view! {
        <div
            class="flex items-start justify-between gap-6"
            style=format!("padding:{}mm {}mm", metrics.padding.vertical, metrics.padding.horizontal)
        >
            <div class="flex flex-col gap-0.5 text-left">{group(Align::Start)}</div>
            <div class="flex flex-col gap-0.5 text-center">{group(Align::Center)}</div>
            <div class="flex flex-col gap-0.5 text-right">{group(Align::End)}</div>
        </div>
    }
    .into_any()
}

/// One value, with its label where it has one.
///
/// A figure sits at the end of its line rather than after its label, so a
/// stack of them - an ageing ladder, four totals - lines up down its own
/// column.
fn field_view<T>(field: &Field<T>, data: &T, metrics: &Metrics) -> AnyView
where
    T: Send + Sync + 'static,
{
    let value = field.value(data);
    let label = field.label.clone().map(|label| {
        view! {
            <span
                class="text-content-subtle"
                style=format!("font-size:{}pt", metrics.type_scale.caption_pt)
            >
                {label}
            </span>
        }
    });

    if field.figures {
        view! {
            <div class="flex items-baseline justify-between gap-6">
                {label}
                <span style=figures_style()>{drawn(value)}</span>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="space-x-2">
                {label}
                <span>{drawn(value)}</span>
            </div>
        }
        .into_any()
    }
}

/// The workspace mark, where the definition places it.
///
/// A workspace that has uploaded none draws its name instead, at the size the
/// title is set in: a letterhead with a hole in it is what makes a document
/// look unfinished, which is the whole reason this item exists. The letterhead
/// is read from context, so it is resolved once for the session rather than
/// once per report or once per page.
fn mark(logo: Logo, metrics: &Metrics) -> AnyView {
    let letterhead = Letterhead::get();
    let height = logo.height_mm;
    let name_pt = metrics.type_scale.title_pt;

    view! {
        <div class="shrink-0">
            {move || match letterhead.get() {
                Some(letterhead) => {
                    match letterhead.logo_src {
                        Some(src) => {
                            view! {
                                <img
                                    src=src
                                    alt=letterhead.name
                                    style=format!("height:{height}mm;width:auto")
                                />
                            }
                                .into_any()
                        }
                        None => {
                            view! {
                                <span
                                    class="font-semibold"
                                    style=format!("font-size:{name_pt}pt")
                                >
                                    {letterhead.name}
                                </span>
                            }
                                .into_any()
                        }
                    }
                }
                None => ().into_any(),
            }}
        </div>
    }
    .into_any()
}

/// The mark on a line of its own, against one edge.
///
/// What a page header gets, and what a letterhead gets when the mark is
/// centred: a wide wordmark in the middle of a line has no room beside it.
fn mark_row(logo: Logo, metrics: &Metrics) -> AnyView {
    view! {
        <div class=format!("flex {}", justify_class(logo.placement.align()))>
            {mark(logo, metrics)}
        </div>
    }
    .into_any()
}

/// The letterhead, arranged around wherever its mark is.
///
/// Against an edge the header's own lines sit alongside the mark, which is
/// what a letterhead with a name and an address beside a logo looks like.
/// Centred, the mark takes a line and everything else goes under it. With no
/// mark at all the header is what it always was.
fn letterhead_layout(logo: Option<Logo>, body: AnyView, metrics: &Metrics) -> AnyView {
    match logo.map(|logo| logo.placement.align()) {
        Some(Align::Start) => view! {
            <div class="flex items-start gap-4">
                {logo.map(|logo| mark(logo, metrics))}
                {body}
            </div>
        }
        .into_any(),
        Some(Align::End) => view! {
            <div class="flex items-start gap-4">
                {body}
                {logo.map(|logo| mark(logo, metrics))}
            </div>
        }
        .into_any(),
        Some(Align::Center) => view! {
            <div>
                {logo.map(|logo| mark_row(logo, metrics))}
                {body}
            </div>
        }
        .into_any(),
        None => body,
    }
}

/// A value, as a link where it names a record and as words where it does not.
///
/// The link is the screen's: it prints as the words it is made of, because a
/// printed page has nowhere to click.
fn drawn(value: Value) -> AnyView {
    let text = value.cell.to_text();

    match value.href {
        Some(href) => view! {
            <A
                href=href
                attr:class="underline decoration-dotted underline-offset-2 hover:decoration-solid print:no-underline"
            >
                {text}
            </A>
        }
        .into_any(),
        None => text.into_any(),
    }
}

/// The detail band: its headings, then each group under them.
fn lines(headings: &[Heading], groups: &[RowGroup], metrics: &Metrics) -> AnyView {
    let headed = headings.iter().any(|heading| heading.label.is_some());

    view! {
        <section>
            {headed
                .then(|| {
                    view! {
                        <div
                            class=format!(
                                "flex font-medium {} {}",
                                rule_ink(metrics.colour),
                                heading_ink(metrics.colour),
                            )
                            style=format!(
                                "border-bottom:{}mm solid;font-size:{}pt",
                                metrics.rules.under_headings,
                                metrics.type_scale.heading_pt,
                            )
                        >
                            {headings
                                .iter()
                                .map(|heading| {
                                    let label = heading.label.clone().unwrap_or_default();

                                    view! {
                                        <div
                                            class=cell_class(heading.align, metrics)
                                            style=cell_style(metrics, false)
                                        >
                                            {label}
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })}

            {groups
                .iter()
                .map(|group| group_view(headings, group, metrics))
                .collect_view()}
        </section>
    }
    .into_any()
}

/// The groups as this page of the report shows them.
///
/// The window is over the rows, not over the groups: a group with nothing on
/// this page is not drawn at all, and one that straddles the edge keeps its
/// header and its subtotal with the part that is here. The subtotal is the
/// group's own, not the page's - it is what the group comes to, and cutting it
/// to the visible rows would be a different number under the same word.
fn paged(groups: &[RowGroup], window: &std::ops::Range<usize>) -> Vec<RowGroup> {
    let mut seen = 0;
    let mut shown = Vec::new();

    for group in groups {
        let start = seen;
        let end = seen + group.rows.len();

        seen = end;

        let from = window.start.max(start);
        let to = window.end.min(end);

        if from >= to {
            continue;
        }

        shown.push(RowGroup {
            label: group.label.clone(),
            rows: group
                .rows
                .get(from - start..to - start)
                .unwrap_or_default()
                .to_vec(),
            totals: group.totals.clone(),
        });
    }

    shown
}

/// One group: what it is called, its rows, and what they come to.
fn group_view(headings: &[Heading], group: &RowGroup, metrics: &Metrics) -> AnyView {
    let header = (!group.label.is_empty()).then(|| {
        view! {
            <div
                class=format!("font-medium {}", heading_ink(metrics.colour))
                style=format!(
                    "min-height:{}mm;padding:{}mm {}mm;font-size:{}pt",
                    metrics.bands.group_header,
                    metrics.padding.vertical,
                    metrics.padding.horizontal,
                    metrics.type_scale.heading_pt,
                )
            >
                {group.label.clone()}
            </div>
        }
    });

    // The subtotal is a row of the same columns, so a figure sits under the
    // column it totals rather than beside a label somewhere else.
    let footer = (!group.totals.is_empty()).then(|| {
        view! {
            <div
                class=format!("flex font-medium {}", rule_ink(metrics.colour))
                style=format!(
                    "min-height:{}mm;border-top:{}mm solid",
                    metrics.bands.group_footer,
                    metrics.rules.above_total,
                )
            >
                {cells(headings, &group.totals, metrics)}
            </div>
        }
    });

    view! {
        {header}
        {group
            .rows
            .iter()
            .map(|row| {
                view! {
                    <div
                        class="flex border-edge"
                        style=format!(
                            "min-height:{}mm;border-bottom:{}mm solid",
                            metrics.bands.detail,
                            metrics.rules.between_rows,
                        )
                    >
                        {cells(headings, row, metrics)}
                    </div>
                }
            })
            .collect_view()}
        {footer}
    }
    .into_any()
}

/// One row's cells, each under the heading it belongs to.
fn cells(headings: &[Heading], row: &[Value], metrics: &Metrics) -> AnyView {
    row.iter()
        .zip(headings)
        .map(|(value, heading)| {
            view! {
                <div
                    class=cell_class(heading.align, metrics)
                    style=cell_style(metrics, heading.figures)
                >
                    {drawn(value.clone())}
                </div>
            }
        })
        .collect_view()
        .into_any()
}

fn cell_class(align: Align, metrics: &Metrics) -> String {
    let edge = if metrics.rules.between_columns > 0.0 {
        "border-r border-edge last:border-r-0"
    } else {
        ""
    };

    format!("min-w-0 flex-1 {} {edge}", align_class(align))
}

fn cell_style(metrics: &Metrics, figures: bool) -> String {
    format!(
        "padding:{}mm {}mm;{}",
        metrics.padding.vertical,
        metrics.padding.horizontal,
        if figures { figures_style() } else { "" },
    )
}

/// What a figure is set in: a stack of faces that are on the machines this
/// runs on, asked for the numerals that line up under each other.
fn figures_style() -> &'static str {
    static STYLE: OnceLock<String> = OnceLock::new();

    STYLE
        .get_or_init(|| {
            format!(
                "font-family:{};font-variant-numeric:{}",
                Typeface::Figures.css_stack(),
                Typeface::Figures.css_numerals(),
            )
        })
        .as_str()
}

/// Which edge of a row a mark sits against.
const fn justify_class(align: Align) -> &'static str {
    match align {
        Align::Start => "justify-start",
        Align::Center => "justify-center",
        Align::End => "justify-end",
    }
}

const fn align_class(align: Align) -> &'static str {
    match align {
        Align::Start => "text-left",
        Align::Center => "text-center",
        Align::End => "text-right",
    }
}

/// What a rule is drawn in. Colour reaches the letterhead and the totals
/// before it reaches anything else.
const fn rule_ink(colour: Colour) -> &'static str {
    match colour {
        Colour::None => "border-edge",
        Colour::Restrained | Colour::Emphasis => "border-brand",
    }
}

/// What a heading is drawn in.
const fn heading_ink(colour: Colour) -> &'static str {
    match colour {
        Colour::None | Colour::Restrained => "text-content",
        Colour::Emphasis => "text-brand",
    }
}

/// How a total is given weight when it cannot be given colour.
const fn weight_ink(colour: Colour) -> &'static str {
    match colour {
        Colour::None => "font-semibold",
        Colour::Restrained | Colour::Emphasis => "font-medium",
    }
}
