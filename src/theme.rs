//! Colour assignment.
//!
//! Each child of the current directory gets its own hue, and depth is encoded as
//! lightness — so a single glance tells you both "which top-level thing is this"
//! and "how deep am I looking". Hues are spread by the golden angle, which keeps
//! neighbours distinguishable for any number of children instead of cycling
//! through a short fixed palette.

use crate::canvas::Rgb;

const GOLDEN_ANGLE: f64 = 137.507_764_05;

/// HSL -> RGB. h in degrees, s and l in 0..=1.
pub fn hsl(h: f64, s: f64, l: f64) -> Rgb {
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Rgb(
        (((r1 + m) * 255.0).round().clamp(0.0, 255.0)) as u8,
        (((g1 + m) * 255.0).round().clamp(0.0, 255.0)) as u8,
        (((b1 + m) * 255.0).round().clamp(0.0, 255.0)) as u8,
    )
}

/// Base hue for the `i`th child of the current directory.
pub fn hue_for(i: usize) -> f64 {
    // Offset so the largest slice isn't a muddy yellow-green.
    200.0 + i as f64 * GOLDEN_ANGLE
}

/// Neutral grey for the aggregate "other" wedge, so it never competes with real data.
pub fn other_color(selected: bool) -> Rgb {
    if selected {
        Rgb(136, 142, 158)
    } else {
        Rgb(74, 79, 92)
    }
}

/// Colour for a wedge standing in for many small siblings within branch `i`.
///
/// Same hue as the branch so it still reads as belonging there, but washed out
/// so it is never mistaken for a directory in its own right.
pub fn aggregate_color(i: usize, depth: usize, selected: bool) -> Rgb {
    let h = hue_for(i);
    let fade = (depth as f64 * 0.05).min(0.20);
    if selected {
        hsl(h, 0.30, (0.58 - fade).max(0.34))
    } else {
        hsl(h, 0.16, (0.40 - fade).max(0.22))
    }
}

/// Colour for a node at `depth` whose top-level ancestor is child `i`.
///
/// `selected` brightens and saturates the whole wedge so the active branch reads
/// as one object across every ring it touches.
pub fn node_color(i: usize, depth: usize, sib: usize, selected: bool) -> Rgb {
    let h = hue_for(i);
    // Deeper rings fade outward, giving the sunburst visible stratification.
    let fade = (depth as f64 * 0.075).min(0.34);
    // Neighbouring siblings share a hue, so nudge lightness in a 3-cycle to keep
    // adjacent wedges from reading as one continuous band.
    let step = [0.0, 0.055, -0.045][sib % 3];
    if selected {
        hsl(h, 0.92, (0.70 - fade + step).clamp(0.35, 0.85))
    } else {
        hsl(h, 0.46, (0.50 - fade + step).clamp(0.18, 0.72))
    }
}
