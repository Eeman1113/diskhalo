//! Render the TUI to a PNG-able PPM so the visuals can be inspected without a
//! live terminal. Dev tool only — not shipped in the binary.
//!
//! usage: cargo run --example preview -- <path> <out.ppm> [sunburst|treemap] [cols] [rows]

use diskhalo::app::{App, View};
use diskhalo::{scan, ui};
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;
use std::env;
use std::io::Write;
use std::path::PathBuf;

const CW: usize = 8; // cell width in px
const CH: usize = 16; // cell height in px — 1:2, so half-blocks come out square

fn rgb(c: Color, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::White => (255, 255, 255),
        Color::Black => (0, 0, 0),
        _ => fallback,
    }
}

/// A tree with `fanout` children per node, `depth` levels deep, all equal size.
fn synthetic(depth: usize, fanout: usize) -> scan::Entry {
    fn build(depth: usize, fanout: usize) -> scan::Entry {
        if depth == 0 {
            return scan::Entry {
                name: "leaf".into(),
                apparent: 1024,
                disk: 1024,
                files: 1,
                is_dir: false,
                denied: false,
                children: vec![],
            };
        }
        let children: Vec<scan::Entry> = (0..fanout).map(|_| build(depth - 1, fanout)).collect();
        let disk = children.iter().map(|c| c.disk).sum();
        scan::Entry {
            name: format!("d{depth}"),
            apparent: disk,
            disk,
            files: children.iter().map(|c| c.files).sum(),
            is_dir: true,
            denied: false,
            children,
        }
    }
    build(depth, fanout)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let path = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| ".".into()));
    let out = args.get(2).cloned().unwrap_or_else(|| "preview.ppm".into());
    let view = match args.get(3).map(|s| s.as_str()) {
        Some("treemap") => View::Treemap,
        _ => View::Sunburst,
    };
    let cols: u16 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(120);
    let rows: u16 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(40);

    // `synthetic` builds a perfectly uniform tree: equal children, uniform depth.
    // Its sunburst must be a flawless filled circle, which makes geometry bugs
    // obvious in a way a real directory's ragged shape never would.
    let (tree, root_for_app, denied) = if path.as_os_str() == "synthetic" {
        (synthetic(4, 6), PathBuf::from("/synthetic"), 0)
    } else {
        let prog = scan::Progress::default();
        let t = scan::scan(&path, scan::Opts::default(), &prog)?;
        let (_, _, _, d) = prog.snapshot();
        (t, path.canonicalize()?, d)
    };

    let mut state = App::new(tree, root_for_app, denied);
    state.view = view;

    let mut term = Terminal::new(TestBackend::new(cols, rows))?;
    term.draw(|f| ui::draw(f, &state))?;
    let buf = term.backend().buffer().clone();

    let (w, h) = (cols as usize * CW, rows as usize * CH);
    let mut img = vec![0u8; w * h * 3];
    let put = |img: &mut Vec<u8>, x: usize, y: usize, c: (u8, u8, u8)| {
        if x < w && y < h {
            let i = (y * w + x) * 3;
            img[i] = c.0;
            img[i + 1] = c.1;
            img[i + 2] = c.2;
        }
    };

    for cy in 0..rows as usize {
        for cx in 0..cols as usize {
            let cell = &buf[(cx as u16, cy as u16)];
            let bg = rgb(cell.bg, (14, 15, 20));
            let fg = rgb(cell.fg, (214, 218, 230));
            let ch = cell.symbol().chars().next().unwrap_or(' ');

            // Background for the whole cell.
            for py in 0..CH {
                for px in 0..CW {
                    put(&mut img, cx * CW + px, cy * CH + py, bg);
                }
            }
            // Block glyphs are exact; everything else gets a bar so text
            // placement is visible without bundling a font.
            let rows_px: Option<(usize, usize)> = match ch {
                '\u{2580}' => Some((0, CH / 2)),  // upper half
                '\u{2584}' => Some((CH / 2, CH)), // lower half
                '\u{2588}' => Some((0, CH)),      // full
                ' ' => None,
                _ => Some((CH / 2 - 2, CH / 2 + 3)), // text stand-in
            };
            if let Some((y0, y1)) = rows_px {
                let inset = if matches!(ch, '\u{2580}' | '\u{2584}' | '\u{2588}') {
                    0
                } else {
                    1
                };
                for py in y0..y1 {
                    for px in inset..CW.saturating_sub(inset) {
                        put(&mut img, cx * CW + px, cy * CH + py, fg);
                    }
                }
            }
        }
    }

    let mut f = std::fs::File::create(&out)?;
    write!(f, "P6\n{w} {h}\n255\n")?;
    f.write_all(&img)?;
    eprintln!("wrote {out} ({w}x{h})");
    Ok(())
}
