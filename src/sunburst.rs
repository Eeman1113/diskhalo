//! Radial (sunburst) layout and rasteriser.
//!
//! This is the view no other terminal disk tool has: DaisyDisk's concentric
//! rings, where each ring is one directory level and arc length is proportional
//! to size. Rendered through the half-block canvas with 2x2 supersampling so the
//! arcs come out smooth rather than staircased.

use crate::canvas::{Canvas, Rgb};
use crate::scan::Entry;
use crate::theme;
use std::f64::consts::TAU;

/// `root_child` value marking the wedge that aggregates all the tiny children.
pub const OTHER: usize = usize::MAX;

/// Rings plus the bookkeeping the UI needs to stay in sync with them.
#[derive(Debug, Clone, Default)]
pub struct Rings {
    /// One entry per visible depth, each sorted by start angle.
    pub rings: Vec<Vec<Seg>>,
    /// Children of the current directory folded into the single "other" wedge.
    pub grouped: Vec<usize>,
    /// Total size represented by that wedge.
    pub other_size: u64,
}

impl Rings {
    /// True when `child` is represented by the aggregate wedge rather than its own.
    pub fn is_grouped(&self, child: usize) -> bool {
        self.grouped.contains(&child)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Seg {
    /// Start angle, radians, clockwise from 12 o'clock.
    pub a0: f64,
    pub a1: f64,
    pub depth: usize,
    /// Index of the top-level child this wedge descends from — drives hue and hit-testing.
    pub root_child: usize,
    /// This wedge stands in for many siblings too small to draw individually.
    pub aggregate: bool,
    /// Index among its own siblings, used to vary lightness so neighbouring
    /// wedges of the same branch don't merge into one indistinguishable band.
    pub sib: usize,
}

/// Build concentric rings from `node`'s children, down to `max_depth` levels.
///
/// Wedges too narrow to occupy a pixel are dropped; without that, deep trees
/// bury the picture under thousands of sub-pixel slivers that render as noise.
/// The threshold is computed per depth from `radius_px`, because an outer ring
/// has far more circumference to spend than an inner one — a flat angular cutoff
/// either erases every deep ring or lets the inner ones turn to hash.
pub fn build(node: &Entry, apparent: bool, max_depth: usize, radius_px: f64, inner: f64) -> Rings {
    let ctx = Ctx {
        apparent,
        max_depth,
        radius_px,
        inner,
    };
    let mut out = Rings {
        rings: vec![Vec::new(); max_depth],
        grouped: Vec::new(),
        other_size: 0,
    };
    let total: u64 = node.children.iter().map(|c| c.size(apparent)).sum();
    if total == 0 {
        return out;
    }

    // Children too thin to be seen or clicked are folded into one wedge. Drawing
    // two hundred 1px slivers reads as visual static and hides the structure that
    // matters; one aggregate wedge is both honest and legible.
    let min0 = min_angle_at(0, max_depth, radius_px, inner);
    let mut visible: Vec<usize> = Vec::new();
    for (i, c) in node.children.iter().enumerate() {
        let span = c.size(apparent) as f64 / total as f64 * TAU;
        if span >= min0 {
            visible.push(i);
        } else {
            out.grouped.push(i);
            out.other_size += c.size(apparent);
        }
    }

    let mut angle = 0.0f64;
    for &i in &visible {
        let child = &node.children[i];
        let span = child.size(apparent) as f64 / total as f64 * TAU;
        let wedge = Wedge {
            a0: angle,
            a1: angle + span,
            depth: 0,
            root_child: i,
            sib: i,
        };
        place(&ctx, child, wedge, &mut out.rings);
        angle += span;
    }

    // The aggregate takes the remaining arc. It spans every ring rather than just
    // the innermost: it stands for a whole swath of the tree, so stopping it at
    // ring 0 would leave a wedge-shaped hole across the widest part of the disc.
    if out.other_size > 0 {
        let span = out.other_size as f64 / total as f64 * TAU;
        for (d, ring) in out.rings.iter_mut().enumerate() {
            ring.push(Seg {
                a0: angle,
                a1: angle + span,
                depth: d,
                root_child: OTHER,
                aggregate: true,
                sib: 0,
            });
        }
    }

    for ring in &mut out.rings {
        ring.sort_unstable_by(|a, b| a.a0.partial_cmp(&b.a0).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Drop outer rings that barely cover anything. They own the most area on the
    // disc but, on a shallow tree, almost none of the data — keeping them starves
    // the complete inner rings of space and leaves the picture mostly black.
    // Aggregates are excluded from the tally: they span every ring by
    // construction, so counting them would keep rings holding no directories.
    const MIN_COVERAGE: f64 = 0.35;
    let keep = out
        .rings
        .iter()
        .enumerate()
        .filter(|(_, ring)| {
            let covered: f64 = ring
                .iter()
                .filter(|s| !s.aggregate)
                .map(|s| s.a1 - s.a0)
                .sum();
            covered >= MIN_COVERAGE * TAU
        })
        .map(|(d, _)| d + 1)
        .max()
        .unwrap_or(1);
    out.rings.truncate(keep);
    out
}

/// Geometry and options shared by every recursive call.
struct Ctx {
    apparent: bool,
    max_depth: usize,
    radius_px: f64,
    inner: f64,
}

/// One wedge's position, carried down the recursion as a unit.
#[derive(Clone, Copy)]
struct Wedge {
    a0: f64,
    a1: f64,
    depth: usize,
    root_child: usize,
    sib: usize,
}

fn place(ctx: &Ctx, node: &Entry, w: Wedge, rings: &mut [Vec<Seg>]) {
    if w.depth >= ctx.max_depth {
        return;
    }
    rings[w.depth].push(Seg {
        a0: w.a0,
        a1: w.a1,
        depth: w.depth,
        root_child: w.root_child,
        aggregate: false,
        sib: w.sib,
    });

    let total: u64 = node.children.iter().map(|c| c.size(ctx.apparent)).sum();
    if total == 0 || w.depth + 1 >= ctx.max_depth {
        return;
    }

    let span = w.a1 - w.a0;
    let min_next = min_angle_at(w.depth + 1, ctx.max_depth, ctx.radius_px, ctx.inner);
    let mut angle = w.a0;
    let mut folded = 0u64;
    let mut drawn = 0usize;

    // Children big enough to see are drawn; the rest are summed. Doing this at
    // every level — not just the first — is what keeps the outer rings complete.
    // Culling them outright instead leaves the rings that own most of the disc's
    // area almost entirely black.
    for child in &node.children {
        let s = child.size(ctx.apparent) as f64 / total as f64 * span;
        if s > min_next {
            let next = Wedge {
                a0: angle,
                a1: angle + s,
                depth: w.depth + 1,
                root_child: w.root_child,
                sib: drawn,
            };
            place(ctx, child, next, rings);
            angle += s;
            drawn += 1;
        } else {
            folded += child.size(ctx.apparent);
        }
    }

    if folded > 0 {
        let s = folded as f64 / total as f64 * span;
        // Keeps the branch's hue so the wedge still reads as part of its parent,
        // but desaturated so it never looks like a directory of its own. Carried
        // out to the rim for the same reason as the top-level aggregate.
        for (d, ring) in rings.iter_mut().enumerate().skip(w.depth + 1) {
            ring.push(Seg {
                a0: angle,
                a1: angle + s,
                depth: d,
                root_child: w.root_child,
                aggregate: true,
                sib: drawn,
            });
        }
    }
}

/// Narrowest wedge worth emitting on ring `depth`, as an angle.
///
/// Derived from arc length: a wedge spanning `theta` at pixel radius `r` draws
/// `theta * r` pixels of arc, so the cutoff has to scale with the ring's radius.
fn min_angle_at(depth: usize, max_depth: usize, radius_px: f64, inner: f64) -> f64 {
    // Two pixels, not one: a one-pixel wedge that recurses several levels draws
    // as a long radial spike, which reads as an artefact rather than as data.
    const MIN_ARC_PX: f64 = 2.0;
    let band = (1.0 - inner) / max_depth as f64;
    let r = (inner + (depth as f64 + 0.5) * band) * radius_px;
    if r <= 1.0 {
        1.0
    } else {
        MIN_ARC_PX / r
    }
}

fn find(ring: &[Seg], theta: f64) -> Option<&Seg> {
    if ring.is_empty() {
        return None;
    }
    // Rightmost segment whose start is <= theta.
    let mut lo = 0usize;
    let mut hi = ring.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if ring[mid].a0 <= theta {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let idx = lo.checked_sub(1)?;
    let seg = &ring[idx];
    if theta < seg.a1 {
        Some(seg)
    } else {
        None
    }
}

/// Rasterise the rings into `canvas`, filling the largest centred circle that fits.
///
/// `inner` is the fraction of the radius left hollow for the centre readout.
/// `selected` is the top-level child index to highlight, if any.
pub fn draw(canvas: &mut Canvas, r: &Rings, inner: f64, selected: Option<usize>, bg: Rgb) {
    let rings = &r.rings;
    let nr = rings.iter().filter(|r| !r.is_empty()).count();
    if nr == 0 {
        return;
    }
    let (w, h) = (canvas.w as f64, canvas.h as f64);
    let cx = w / 2.0;
    let cy = h / 2.0;
    // Pixels are square-ish already (half-blocks), so one radius serves both axes.
    let radius = (w / 2.0).min(h / 2.0) - 0.5;
    if radius <= 1.0 {
        return;
    }
    let band = (1.0 - inner) / nr as f64;
    // A hairline of unpainted space between rings reads as a border for free.
    // Fixed in pixels, not as a fraction of the band: a proportional gap turns
    // into a fat dark donut whenever there are only a couple of thick rings.
    let gap_r = (0.6 / radius).min(band * 0.3);

    const SS: usize = 2; // 2x2 supersampling
    for py in 0..canvas.h {
        for px in 0..canvas.w {
            let mut acc = (0u32, 0u32, 0u32);
            let mut hits = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = px as f64 + (sx as f64 + 0.5) / SS as f64;
                    let fy = py as f64 + (sy as f64 + 0.5) / SS as f64;
                    let dx = fx - cx;
                    let dy = fy - cy;
                    let r = (dx * dx + dy * dy).sqrt() / radius;
                    if r > 1.0 || r < inner {
                        continue;
                    }
                    let t = (r - inner) / (1.0 - inner);
                    let pos = t * nr as f64;
                    let depth = (pos.floor() as usize).min(nr - 1);
                    let frac = pos - depth as f64;
                    if frac * band < gap_r || (1.0 - frac) * band < gap_r {
                        continue; // ring separator
                    }
                    // Clockwise from 12 o'clock, matching how people read a pie.
                    let theta = dx.atan2(-dy).rem_euclid(TAU);
                    // A hairline between neighbouring wedges, sized in pixels at
                    // this radius so it stays one pixel wide at every depth.
                    let r_px = r * radius;
                    let gap_theta = if r_px > 1.0 { 0.55 / r_px } else { 0.0 };
                    if let Some(seg) = find(&rings[depth], theta) {
                        if theta - seg.a0 < gap_theta || seg.a1 - theta < gap_theta {
                            continue;
                        }
                        let sel = selected == Some(seg.root_child);
                        let c = if seg.root_child == OTHER {
                            theme::other_color(sel)
                        } else if seg.aggregate {
                            theme::aggregate_color(seg.root_child, seg.depth, sel)
                        } else {
                            theme::node_color(seg.root_child, seg.depth, seg.sib, sel)
                        };
                        acc.0 += c.0 as u32;
                        acc.1 += c.1 as u32;
                        acc.2 += c.2 as u32;
                        hits += 1;
                    }
                }
            }
            if hits > 0 {
                // Mean of the samples that actually landed on a wedge gives the
                // true colour; coverage then alpha-blends it against the real
                // background. Averaging over all samples instead would darken
                // every pixel touching a ring separator and speckle the interior.
                let n = f64::from(hits);
                let cov = n / (SS * SS) as f64;
                let mix = |sum: u32, bgc: u8| -> u8 {
                    let fgc = f64::from(sum) / n;
                    (f64::from(bgc) + (fgc - f64::from(bgc)) * cov)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                canvas.set(
                    px,
                    py,
                    Rgb(mix(acc.0, bg.0), mix(acc.1, bg.1), mix(acc.2, bg.2)),
                );
            }
        }
    }
}

/// Which top-level child sits under a canvas pixel, for mouse hit-testing.
pub fn hit_test(
    canvas_w: usize,
    canvas_h: usize,
    r: &Rings,
    inner: f64,
    px: f64,
    py: f64,
) -> Option<usize> {
    let rings = &r.rings;
    let nr = rings.iter().filter(|r| !r.is_empty()).count();
    if nr == 0 {
        return None;
    }
    let (w, h) = (canvas_w as f64, canvas_h as f64);
    let radius = (w / 2.0).min(h / 2.0) - 0.5;
    let dx = px - w / 2.0;
    let dy = py - h / 2.0;
    let r = (dx * dx + dy * dy).sqrt() / radius;
    if r > 1.0 || r < inner {
        return None;
    }
    let depth = (((r - inner) / (1.0 - inner) * nr as f64).floor() as usize).min(nr - 1);
    let theta = dx.atan2(-dy).rem_euclid(TAU);
    find(&rings[depth], theta)
        .map(|s| s.root_child)
        .filter(|&c| c != OTHER)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, size: u64) -> Entry {
        Entry {
            name: name.into(),
            apparent: size,
            disk: size,
            files: 1,
            is_dir: false,
            denied: false,
            children: vec![],
        }
    }

    #[test]
    fn first_ring_spans_full_circle() {
        let root = Entry {
            name: "root".into(),
            apparent: 100,
            disk: 100,
            files: 3,
            is_dir: true,
            denied: false,
            children: vec![leaf("a", 50), leaf("b", 30), leaf("c", 20)],
        };
        let r = build(&root, false, 4, 1000.0, 0.3);
        let covered: f64 = r.rings[0].iter().map(|s| s.a1 - s.a0).sum();
        assert!((covered - TAU).abs() < 1e-9, "ring 0 covered {covered}");
        assert_eq!(r.rings[0].len(), 3);
    }

    #[test]
    fn arc_length_is_proportional_to_size() {
        let root = Entry {
            name: "root".into(),
            apparent: 100,
            disk: 100,
            files: 2,
            is_dir: true,
            denied: false,
            children: vec![leaf("big", 75), leaf("small", 25)],
        };
        let r = build(&root, false, 2, 1000.0, 0.3);
        let big = r.rings[0].iter().find(|s| s.root_child == 0).unwrap();
        assert!(((big.a1 - big.a0) - TAU * 0.75).abs() < 1e-9);
    }

    #[test]
    fn find_locates_and_rejects_gaps() {
        let ring = vec![
            Seg {
                a0: 0.0,
                a1: 1.0,
                depth: 0,
                root_child: 0,
                aggregate: false,
                sib: 0,
            },
            Seg {
                a0: 2.0,
                a1: 3.0,
                depth: 0,
                root_child: 1,
                aggregate: false,
                sib: 1,
            },
        ];
        assert_eq!(find(&ring, 0.5).unwrap().root_child, 0);
        assert_eq!(find(&ring, 2.5).unwrap().root_child, 1);
        assert!(
            find(&ring, 1.5).is_none(),
            "gap between segments must not match"
        );
        assert!(find(&ring, 3.5).is_none(), "past the end must not match");
    }

    #[test]
    fn empty_tree_is_harmless() {
        let root = Entry {
            name: "e".into(),
            apparent: 0,
            disk: 0,
            files: 0,
            is_dir: true,
            denied: false,
            children: vec![],
        };
        let r = build(&root, false, 3, 1000.0, 0.3);
        assert!(r.rings.iter().all(|x| x.is_empty()));
        let mut c = Canvas::new(10, 5);
        draw(&mut c, &r, 0.3, None, Rgb(0, 0, 0)); // must not panic
    }
}
