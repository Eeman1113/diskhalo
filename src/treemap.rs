//! Squarified treemap layout (Bruls, Huizing & van Wijk, 2000).
//!
//! Lays out weighted items as nested rectangles whose areas are proportional to
//! their weights, preferring near-square aspect ratios so small slivers stay
//! clickable and readable.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }
    pub fn area(&self) -> f64 {
        self.w * self.h
    }
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Worst (largest) aspect ratio produced by laying `row` along a side of length `side`.
fn worst(row: &[f64], side: f64) -> f64 {
    let s: f64 = row.iter().sum();
    if s <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let rmax = row.iter().copied().fold(f64::MIN, f64::max);
    let rmin = row.iter().copied().fold(f64::MAX, f64::min);
    if rmin <= 0.0 {
        return f64::INFINITY;
    }
    let s2 = s * s;
    let side2 = side * side;
    (side2 * rmax / s2).max(s2 / (side2 * rmin))
}

/// Lay out `areas` (already scaled so their sum equals `rect.area()`) inside `rect`.
///
/// Returns one rectangle per input, in input order.
pub fn squarify(areas: &[f64], rect: Rect) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::with_capacity(areas.len());
    let mut r = rect;
    let mut i = 0;

    while i < areas.len() {
        // Zero-weight tails get collapsed rectangles rather than NaNs.
        if areas[i] <= 0.0 {
            out.push(Rect::new(r.x, r.y, 0.0, 0.0));
            i += 1;
            continue;
        }

        let side = r.w.min(r.h);
        // Grow the row while the aspect ratio keeps improving.
        let mut j = i + 1;
        let mut cur = worst(&areas[i..j], side);
        while j < areas.len() && areas[j] > 0.0 {
            let next = worst(&areas[i..j + 1], side);
            if next > cur {
                break;
            }
            cur = next;
            j += 1;
        }

        let row = &areas[i..j];
        let total: f64 = row.iter().sum();

        if r.w >= r.h {
            // Row becomes a vertical strip down the left edge.
            let strip_w = if r.h > 0.0 { total / r.h } else { 0.0 };
            let mut y = r.y;
            for &a in row {
                let h = if total > 0.0 { a / total * r.h } else { 0.0 };
                out.push(Rect::new(r.x, y, strip_w, h));
                y += h;
            }
            r.x += strip_w;
            r.w = (r.w - strip_w).max(0.0);
        } else {
            // Row becomes a horizontal strip across the top edge.
            let strip_h = if r.w > 0.0 { total / r.w } else { 0.0 };
            let mut x = r.x;
            for &a in row {
                let w = if total > 0.0 { a / total * r.w } else { 0.0 };
                out.push(Rect::new(x, r.y, w, strip_h));
                x += w;
            }
            r.y += strip_h;
            r.h = (r.h - strip_h).max(0.0);
        }
        i = j;
    }
    out
}

/// Convenience: lay out raw weights in `rect`, scaling them to fill it.
pub fn layout_weights(weights: &[u64], rect: Rect) -> Vec<Rect> {
    let total: u64 = weights.iter().sum();
    if total == 0 || rect.area() <= 0.0 {
        return weights
            .iter()
            .map(|_| Rect::new(rect.x, rect.y, 0.0, 0.0))
            .collect();
    }
    let scale = rect.area() / total as f64;
    let areas: Vec<f64> = weights.iter().map(|&w| w as f64 * scale).collect();
    squarify(&areas, rect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_are_proportional() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let out = layout_weights(&[50, 30, 20], rect);
        assert_eq!(out.len(), 3);
        // Each rectangle's area should match its share of the total, within fp slop.
        for (r, &w) in out.iter().zip(&[50u64, 30, 20]) {
            let expected = rect.area() * w as f64 / 100.0;
            assert!(
                (r.area() - expected).abs() < 1.0,
                "got {} want {}",
                r.area(),
                expected
            );
        }
    }

    #[test]
    fn rectangles_stay_inside_and_tile() {
        let rect = Rect::new(0.0, 0.0, 64.0, 40.0);
        let out = layout_weights(&[9, 7, 6, 5, 4, 3, 2, 1], rect);
        let covered: f64 = out.iter().map(|r| r.area()).sum();
        assert!(
            (covered - rect.area()).abs() < 1.0,
            "tiling lost area: {covered}"
        );
        for r in &out {
            assert!(r.x >= -0.001 && r.y >= -0.001);
            assert!(r.x + r.w <= rect.w + 0.001 && r.y + r.h <= rect.h + 0.001);
        }
    }

    #[test]
    fn handles_empty_and_zero() {
        assert!(layout_weights(&[], Rect::new(0.0, 0.0, 10.0, 10.0)).is_empty());
        let out = layout_weights(&[0, 0], Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(out.len(), 2);
    }
}
