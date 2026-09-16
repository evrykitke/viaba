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
//! Group bands are drawn once, around the rows; drawing one per group is what
//! grouping adds.

use leptos::prelude::*;
use phonix_core::report::{Align, BandKind, Colour, Metrics};

use super::definition::Content;
use super::{Band, Field, Heading, ReportDefinition};
use crate::ui::table::Cell;

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
    let metrics = definition.theme.metrics();
    let page = definition.page;

    let sheet = format!(
        "width:{}mm;padding:{}mm {}mm {}mm {}mm;font-size:{}pt",
        page.width_mm(),
        page.margins.top,
        page.margins.right,
        page.margins.bottom,
        page.margins.left,
        metrics.type_scale.body_pt,
    );

    let letterhead = definition.band_of(BandKind::ReportHeader).map(|band| {
        view! {
            <header
                class=format!("block {}", rule_ink(metrics.colour))
                style=format!(
                    "min-height:{}mm;border-bottom:{}mm solid",
                    metrics.bands.report_header,
                    metrics.rules.under_letterhead,
                )
            >
                <h1
                    class=heading_ink(metrics.colour)
                    style=format!("font-size:{}pt", metrics.type_scale.title_pt)
                >
                    {definition.title.clone()}
                </h1>
                {band_content(band, &data, &metrics)}
            </header>
        }
    });

    let page_header = definition
        .band_of(BandKind::PageHeader)
        .map(|band| running_band(band, &data, &metrics, BandKind::PageHeader));
    let page_footer = definition
        .band_of(BandKind::PageFooter)
        .map(|band| running_band(band, &data, &metrics, BandKind::PageFooter));
    let group_header = definition
        .band_of(BandKind::GroupHeader)
        .map(|band| running_band(band, &data, &metrics, BandKind::GroupHeader));
    let group_footer = definition
        .band_of(BandKind::GroupFooter)
        .map(|band| running_band(band, &data, &metrics, BandKind::GroupFooter));

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
                {band_content(band, &data, &metrics)}
            </footer>
        }
    });

    let detail = definition
        .band_of(BandKind::Detail)
        .map(|band| band_content(band, &data, &metrics));

    view! {
        <article class="mx-auto bg-surface text-content shadow-card" style=sheet>
            {letterhead}
            {page_header}
            {group_header}
            {detail}
            {group_footer}
            {footer}
            {page_footer}
        </article>
    }
}

/// A band with no rule of its own: the page bands and, until grouping lands,
/// the group bands.
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
fn field_view<T>(field: &Field<T>, data: &T, metrics: &Metrics) -> AnyView
where
    T: Send + Sync + 'static,
{
    let value = field.read(data).to_text();

    view! {
        <div>
            {field
                .label
                .clone()
                .map(|label| {
                    view! {
                        <span
                            class="mr-2 text-content-subtle"
                            style=format!("font-size:{}pt", metrics.type_scale.caption_pt)
                        >
                            {label}
                        </span>
                    }
                })}
            <span>{value}</span>
        </div>
    }
    .into_any()
}

/// The detail band: its headings, then a row per line.
fn lines(headings: &[Heading], rows: &[Vec<Cell>], metrics: &Metrics) -> AnyView {
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
                                            style=cell_style(metrics)
                                        >
                                            {label}
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })}

            {rows
                .iter()
                .map(|cells| {
                    let cells = cells
                        .iter()
                        .zip(headings)
                        .map(|(cell, heading)| {
                            view! {
                                <div
                                    class=cell_class(heading.align, metrics)
                                    style=cell_style(metrics)
                                >
                                    {cell.to_text()}
                                </div>
                            }
                        })
                        .collect_view();

                    view! {
                        <div
                            class="flex border-edge"
                            style=format!(
                                "min-height:{}mm;border-bottom:{}mm solid",
                                metrics.bands.detail,
                                metrics.rules.between_rows,
                            )
                        >
                            {cells}
                        </div>
                    }
                })
                .collect_view()}
        </section>
    }
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

fn cell_style(metrics: &Metrics) -> String {
    format!(
        "padding:{}mm {}mm",
        metrics.padding.vertical, metrics.padding.horizontal
    )
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
