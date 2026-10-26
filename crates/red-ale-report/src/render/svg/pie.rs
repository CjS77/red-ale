//! Pie charts: one slice per value, its angle the exact fraction of the total, its edge points from the CORDIC.

use super::cordic::{scale, sin_cos_turn};
use super::geom::{Hu, UNIT, fmt};
use super::{Frame, hover};
use crate::number::{Rational, SizeCap};
use crate::render::text::escape;
use crate::result::Chart;

/// A slice as drawn: its edge points on the circle, from 12 o'clock clockwise as Chart.js starts
/// (`rotation: -0.5π`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Slice {
    pub index: usize,
    pub start: (Hu, Hu),
    pub end: (Hu, Hu),
    /// The slice is more than half the circle.
    pub large: bool,
    /// The slice is the whole circle.
    pub whole: bool,
}

/// The point at a fraction `f` of a turn on the circle of radius `r` about (cx, cy).
fn on_circle(f: &Rational, (cx, cy): (Hu, Hu), r: Hu) -> (Hu, Hu) {
    let (sin, cos) = sin_cos_turn(f);
    (cx + scale(r, sin), cy - scale(r, cos))
}

/// The slices of `values`: each value's share of the total of their absolute values, as Chart.js sizes a slice by
/// `|value| / total`. Zero and missing values have no slice.
pub(super) fn slices(values: &[Option<Rational>], centre: (Hu, Hu), r: Hu) -> Vec<Slice> {
    let sizes: Vec<Rational> = values.iter().map(|v| v.as_ref().map(Rational::abs).unwrap_or_default()).collect();
    let total = sizes.iter().fold(Rational::zero(), |sum, v| &sum + v);
    if total.is_zero() {
        return Vec::new();
    }
    let share = |v: &Rational| v.checked_div(&total, SizeCap::bits(u64::MAX)).unwrap_or_default();
    let half = Rational::new(1, 2).unwrap_or_default();
    sizes
        .iter()
        .enumerate()
        .scan(Rational::zero(), |done, (index, size)| {
            let from = done.clone();
            *done = &*done + size;
            Some((index, share(&from), share(done), size.is_zero()))
        })
        .filter(|(_, _, _, empty)| !empty)
        .map(|(index, from, to, _)| {
            let span = &to - &from;
            Slice { index, start: on_circle(&from, centre, r), end: on_circle(&to, centre, r), large: span > half, whole: span == Rational::one() }
        })
        .collect()
}

pub(super) fn draw(chart: &Chart, frame: &Frame) -> String {
    let area = frame.area;
    let centre = (area.x0 + area.width() / 2, area.y0 + area.height() / 2);
    let r = (area.width().min(area.height()) / 2 - 4 * UNIT).max(UNIT);
    let Some(series) = chart.series.first() else {
        return outline(centre, r);
    };
    let values = series.values.as_deref().unwrap_or_default();
    let drawn = slices(values, centre, r);
    if drawn.is_empty() {
        return outline(centre, r);
    }
    let colors = series.colors.as_deref().unwrap_or_default();
    drawn
        .iter()
        .map(|s| {
            let color = escape(colors.get(s.index).unwrap_or(&series.color).as_str());
            let title = hover(chart, s.index, 0);
            if s.whole {
                return format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{color}\">{title}</circle>", fmt(centre.0), fmt(centre.1), fmt(r));
            }
            let d = format!(
                "M{} {} L{} {} A{} {} 0 {} 1 {} {} Z",
                fmt(centre.0),
                fmt(centre.1),
                fmt(s.start.0),
                fmt(s.start.1),
                fmt(r),
                fmt(r),
                u8::from(s.large),
                fmt(s.end.0),
                fmt(s.end.1)
            );
            format!("<path d=\"{d}\" fill=\"{color}\" stroke=\"#ffffff\" stroke-width=\"1\">{title}</path>")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// An empty circle for a pie with nothing to show.
fn outline((cx, cy): (Hu, Hu), r: Hu) -> String {
    format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"#c8c8c8\"/>", fmt(cx), fmt(cy), fmt(r))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const CENTRE: (Hu, Hu) = (50_000, 30_000);
    const R: Hu = 20_000;

    fn values(v: &[i64]) -> Vec<Option<Rational>> {
        v.iter().map(|n| Some(Rational::from_int(*n))).collect()
    }

    #[test]
    fn slices_start_at_twelve_o_clock_and_go_clockwise() {
        let s = slices(&values(&[1, 1, 2]), CENTRE, R);
        assert_eq!(s.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>(), [
            ((50_000, 10_000), (70_000, 30_000)),
            ((70_000, 30_000), (50_000, 50_000)),
            ((50_000, 50_000), (50_000, 10_000))
        ]);
        assert!(s.iter().all(|s| !s.large && !s.whole));
        let one = slices(&[None, Some(Rational::from_int(-3)), Some(Rational::zero())], CENTRE, R);
        assert_eq!((one.len(), one[0].index, one[0].whole), (1, 1, true));
        assert!(slices(&values(&[0, 0]), CENTRE, R).is_empty());
        assert!(slices(&values(&[3, 1]), CENTRE, R)[0].large);
    }

    proptest! {
        /// The slices chain around the whole circle: each starts where the last ended, the first at 12 o'clock and
        /// the last back there, and every edge point is on the circle to within rounding.
        #[test]
        fn slices_cover_the_full_circle(v in proptest::collection::vec(-1_000_000_i64..1_000_000, 1..40)) {
            let s = slices(&values(&v), CENTRE, R);
            if v.iter().all(|n| *n == 0) {
                prop_assert!(s.is_empty());
                return Ok(());
            }
            let top = (CENTRE.0, CENTRE.1 - R);
            prop_assert_eq!(s[0].start, top);
            prop_assert_eq!(s[s.len() - 1].end, top);
            prop_assert!(s.windows(2).all(|w| w[0].end == w[1].start));
            prop_assert!(s.iter().filter(|s| s.large).count() <= 1);
            for p in s.iter().flat_map(|s| [s.start, s.end]) {
                let (dx, dy) = (i128::from(p.0 - CENTRE.0), i128::from(p.1 - CENTRE.1));
                let r2 = i128::from(R) * i128::from(R);
                prop_assert!((dx * dx + dy * dy - r2).abs() <= 2 * i128::from(R) + 2);
            }
        }
    }
}
