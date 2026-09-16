//! What a chart band is, as numbers rather than as a drawing.
//!
//! The report draws these as inline SVG and the browser prints that SVG, so
//! there is one picture rather than two drawings of one idea - ADR 0008 §7.
//! What is here is the arithmetic: the scale, the ticks, and where every mark
//! goes in a box of a known size. It is pure, so it is tested, which is the
//! same reason the band model is here and not in the crate that draws it.
//!
//! Units are the drawing's own - a viewBox of 100 by whatever the chart's
//! aspect gives - and the sheet scales it. Nothing here is a pixel.

use serde::{Deserialize, Serialize};

/// How many ticks a value axis aims for. Four to six is what a report band is
/// tall enough to label.
const TICKS: f64 = 5.0;

/// The shape a chart takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// Upright bars, one group per point. The default for a magnitude read
    /// against a category.
    Column,
    /// Bars on their side, for categories whose names are too long to stand
    /// under a column.
    Bar,
    /// One column per point, its series stacked, for parts of a whole that is
    /// itself worth comparing.
    StackedColumn,
    Line,
    Area,
    Pie,
    Donut,
}

impl ChartKind {
    /// Whether the value axis runs across the box rather than up it.
    pub const fn horizontal(self) -> bool {
        matches!(self, Self::Bar)
    }

    /// Whether the kind has axes at all.
    pub const fn axes(self) -> bool {
        !matches!(self, Self::Pie | Self::Donut)
    }

    /// Whether every point's series are added together rather than compared.
    pub const fn stacks(self) -> bool {
        matches!(self, Self::StackedColumn)
    }
}

/// One point of a chart: what it is called, and one value per series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub label: String,
    pub values: Vec<f64>,
}

impl Point {
    pub fn new(label: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            label: label.into(),
            values,
        }
    }

    /// What this point comes to across its series.
    pub fn total(&self) -> f64 {
        self.values.iter().copied().filter(|v| v.is_finite()).sum()
    }
}

/// Where one mark is drawn, in the box's own units.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A bar or a column.
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        series: usize,
        /// The point it belongs to, which is what a label is read from.
        point: usize,
    },
    /// A line, or the outline of an area.
    Path {
        points: Vec<(f32, f32)>,
        series: usize,
        /// Whether it is closed down to the baseline and filled.
        filled: bool,
    },
    /// One segment of a pie or a donut, as the two angles it spans in degrees
    /// clockwise from twelve o'clock.
    Slice {
        from: f32,
        to: f32,
        series: usize,
        point: usize,
    },
}

/// One mark on the value axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub value: f64,
    /// Where it sits along the axis, in the box's units.
    pub at: f32,
}

/// A chart, placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Plot {
    pub kind: ChartKind,
    pub width: f32,
    pub height: f32,
    /// The box the marks are drawn in, inside the labels.
    pub plot: Frame,
    pub marks: Vec<Mark>,
    pub ticks: Vec<Tick>,
    /// The points, in the order the marks name them.
    pub labels: Vec<String>,
}

/// A rectangle in the drawing's own units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Frame {
    pub const fn right(&self) -> f32 {
        self.x + self.width
    }

    pub const fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Lay a chart out in a box `width` by `height`.
///
/// Empty of points, or of values that are all zero, still returns a plot: an
/// axis with a zero on it is a chart that says there is nothing, which is an
/// answer. A caller drawing nothing at all would leave a hole in the report.
pub fn plot(kind: ChartKind, points: &[Point], width: f32, height: f32) -> Plot {
    let labelled = kind.axes();

    // Room for the tick labels and the category labels. Enough for five
    // characters of a figure at the size a report caption is set in.
    let gutter = if labelled { 12.0 } else { 0.0 };
    let footer = if labelled { 8.0 } else { 0.0 };

    let frame = Frame {
        x: if kind.horizontal() {
            gutter * 1.6
        } else {
            gutter
        },
        y: 2.0,
        width: width
            - if kind.horizontal() {
                gutter * 1.6
            } else {
                gutter
            }
            - 2.0,
        height: height - footer - 2.0,
    };

    let top = ceiling(points, kind);
    let ticks = if labelled {
        ticks(top, kind, frame)
    } else {
        Vec::new()
    };

    let marks = match kind {
        ChartKind::Column | ChartKind::Bar | ChartKind::StackedColumn => {
            bars(kind, points, frame, top)
        }
        ChartKind::Line | ChartKind::Area => lines(kind, points, frame, top),
        ChartKind::Pie | ChartKind::Donut => slices(points, frame),
    };

    Plot {
        kind,
        width,
        height,
        plot: frame,
        marks,
        ticks,
        labels: points.iter().map(|point| point.label.clone()).collect(),
    }
}

/// The value the axis runs to: a round number above the largest thing drawn.
fn ceiling(points: &[Point], kind: ChartKind) -> f64 {
    let largest = points
        .iter()
        .map(|point| {
            if kind.stacks() {
                point.total()
            } else {
                point
                    .values
                    .iter()
                    .copied()
                    .filter(|value| value.is_finite())
                    .fold(0.0_f64, f64::max)
            }
        })
        .fold(0.0_f64, f64::max);

    if largest <= 0.0 {
        return 1.0;
    }

    let step = step_for(largest);

    (largest / step).ceil() * step
}

/// A step a reader can add up in their head: one, two or five, times a power
/// of ten.
fn step_for(largest: f64) -> f64 {
    let rough = largest / TICKS;

    if rough <= 0.0 {
        return 1.0;
    }

    let magnitude = 10.0_f64.powf(rough.log10().floor());
    let normalised = rough / magnitude;

    let snapped = if normalised <= 1.0 {
        1.0
    } else if normalised <= 2.0 {
        2.0
    } else if normalised <= 5.0 {
        5.0
    } else {
        10.0
    };

    snapped * magnitude
}

/// The ticks up the value axis, including nought and the ceiling.
fn ticks(top: f64, kind: ChartKind, frame: Frame) -> Vec<Tick> {
    let step = step_for(top);
    let mut ticks = Vec::new();
    let mut value = 0.0_f64;

    while value <= top + step / 2.0 {
        ticks.push(Tick {
            value,
            at: along(value, top, kind, frame),
        });

        value += step;
    }

    ticks
}

/// Where a value sits on the axis.
fn along(value: f64, top: f64, kind: ChartKind, frame: Frame) -> f32 {
    let share = if top > 0.0 { (value / top) as f32 } else { 0.0 };

    if kind.horizontal() {
        frame.x + frame.width * share
    } else {
        frame.bottom() - frame.height * share
    }
}

/// Columns, bars, and stacks of either.
fn bars(kind: ChartKind, points: &[Point], frame: Frame, top: f64) -> Vec<Mark> {
    let count = points.len().max(1);
    let series = points.first().map_or(1, |point| point.values.len().max(1));
    // A gap the surface shows through, between one point's bars and the next.
    let slot = span(kind, frame) / count as f32;
    let bar = (slot * 0.72).max(0.5);
    let each = if kind.stacks() {
        bar
    } else {
        (bar / series as f32).max(0.3)
    };

    let mut marks = Vec::new();

    for (index, point) in points.iter().enumerate() {
        let start = slot * index as f32 + (slot - bar) / 2.0;
        let mut stacked = 0.0_f64;

        for (which, value) in point.values.iter().copied().enumerate() {
            if !value.is_finite() || value <= 0.0 {
                continue;
            }

            let (from, to) = if kind.stacks() {
                let base = stacked;
                stacked += value;
                (base, stacked)
            } else {
                (0.0, value)
            };

            let low = along(from, top, kind, frame);
            let high = along(to, top, kind, frame);
            let offset = if kind.stacks() {
                0.0
            } else {
                each * which as f32
            };

            marks.push(if kind.horizontal() {
                Mark::Rect {
                    x: low.min(high),
                    y: frame.y + start + offset,
                    width: (high - low).abs(),
                    height: each,
                    series: which,
                    point: index,
                }
            } else {
                Mark::Rect {
                    x: frame.x + start + offset,
                    y: low.min(high),
                    width: each,
                    height: (high - low).abs(),
                    series: which,
                    point: index,
                }
            });
        }
    }

    marks
}

/// How much room the category axis has.
fn span(kind: ChartKind, frame: Frame) -> f32 {
    if kind.horizontal() {
        frame.height
    } else {
        frame.width
    }
}

/// A line per series, and the area under it where the kind asks for one.
fn lines(kind: ChartKind, points: &[Point], frame: Frame, top: f64) -> Vec<Mark> {
    let series = points.first().map_or(0, |point| point.values.len());
    let steps = points.len().saturating_sub(1).max(1) as f32;
    let mut marks = Vec::new();

    for which in 0..series {
        let drawn: Vec<(f32, f32)> = points
            .iter()
            .enumerate()
            .filter_map(|(index, point)| {
                let value = point.values.get(which).copied()?;

                value.is_finite().then(|| {
                    (
                        frame.x + frame.width * (index as f32 / steps),
                        along(value, top, kind, frame),
                    )
                })
            })
            .collect();

        if drawn.len() < 2 {
            continue;
        }

        marks.push(Mark::Path {
            points: drawn,
            series: which,
            filled: matches!(kind, ChartKind::Area),
        });
    }

    marks
}

/// A slice per point, of the first series only.
///
/// A pie of two series is two pies, and this draws one: parts of one whole is
/// the only thing the shape can say.
fn slices(points: &[Point], _frame: Frame) -> Vec<Mark> {
    let whole: f64 = points
        .iter()
        .map(|point| point.values.first().copied().unwrap_or_default())
        .filter(|value| value.is_finite() && *value > 0.0)
        .sum();

    if whole <= 0.0 {
        return Vec::new();
    }

    let mut marks = Vec::new();
    let mut from = 0.0_f32;

    for (index, point) in points.iter().enumerate() {
        let value = point.values.first().copied().unwrap_or_default();

        if !value.is_finite() || value <= 0.0 {
            continue;
        }

        let sweep = ((value / whole) * 360.0) as f32;

        marks.push(Mark::Slice {
            from,
            to: from + sweep,
            // A pie is coloured by point, not by series: the slices are the
            // categories.
            series: index,
            point: index,
        });

        from += sweep;
    }

    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(values: &[(&str, f64)]) -> Vec<Point> {
        values
            .iter()
            .map(|(label, value)| Point::new(*label, vec![*value]))
            .collect()
    }

    #[test]
    fn an_axis_runs_to_a_round_number_above_the_data() {
        assert_eq!(ceiling(&points(&[("a", 7.0)]), ChartKind::Column), 8.0);
        assert_eq!(
            ceiling(&points(&[("a", 4200.0)]), ChartKind::Column),
            5000.0
        );
        assert!((ceiling(&points(&[("a", 0.3)]), ChartKind::Column) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn a_step_is_one_two_or_five_times_a_power_of_ten() {
        for largest in [3.0, 17.0, 480.0, 9_100.0, 0.04] {
            let step = step_for(largest);
            let magnitude = 10.0_f64.powf(step.log10().floor());
            let normalised = step / magnitude;

            assert!(
                (normalised - 1.0).abs() < 1e-9
                    || (normalised - 2.0).abs() < 1e-9
                    || (normalised - 5.0).abs() < 1e-9,
                "{largest} gave a step of {step}",
            );
        }
    }

    #[test]
    fn a_chart_of_nothing_is_still_a_chart() {
        let drawn = plot(ChartKind::Column, &[], 100.0, 40.0);

        assert!(drawn.marks.is_empty());
        assert!(!drawn.ticks.is_empty(), "an axis saying nought");
    }

    #[test]
    fn a_column_stands_on_the_baseline() {
        let drawn = plot(ChartKind::Column, &points(&[("a", 10.0)]), 100.0, 40.0);

        let Some(Mark::Rect { y, height, .. }) = drawn.marks.first() else {
            panic!("a column");
        };

        assert!(
            (y + height - drawn.plot.bottom()).abs() < 0.01,
            "the column does not reach the baseline",
        );
    }

    #[test]
    fn a_stack_is_as_tall_as_its_parts() {
        let stacked = vec![Point::new("a", vec![3.0, 7.0])];
        let drawn = plot(ChartKind::StackedColumn, &stacked, 100.0, 40.0);

        let total: f32 = drawn
            .marks
            .iter()
            .map(|mark| match mark {
                Mark::Rect { height, .. } => *height,
                _ => 0.0,
            })
            .sum();

        // Ten of a ceiling of ten, so the stack fills the plot.
        assert!(
            (total - drawn.plot.height).abs() < 0.01,
            "the stack is {total} of {}",
            drawn.plot.height,
        );
    }

    #[test]
    fn slices_come_to_a_whole_turn() {
        let drawn = plot(
            ChartKind::Pie,
            &points(&[("a", 1.0), ("b", 2.0), ("c", 1.0)]),
            100.0,
            100.0,
        );

        let Some(Mark::Slice { to, .. }) = drawn.marks.last() else {
            panic!("three slices");
        };

        assert!((to - 360.0).abs() < 0.01, "the pie ends at {to} degrees");
    }

    #[test]
    fn a_line_needs_two_points_to_be_a_line() {
        let one = plot(ChartKind::Line, &points(&[("a", 1.0)]), 100.0, 40.0);
        let two = plot(
            ChartKind::Line,
            &points(&[("a", 1.0), ("b", 2.0)]),
            100.0,
            40.0,
        );

        assert!(one.marks.is_empty());
        assert_eq!(two.marks.len(), 1);
    }

    #[test]
    fn a_bar_runs_across_rather_than_up() {
        let drawn = plot(ChartKind::Bar, &points(&[("a", 10.0)]), 100.0, 40.0);

        let Some(Mark::Rect { x, width, .. }) = drawn.marks.first() else {
            panic!("a bar");
        };

        assert!((x - drawn.plot.x).abs() < 0.01, "a bar starts at the axis");
        assert!(*width > 0.0);
    }
}
