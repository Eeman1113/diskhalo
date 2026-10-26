//! diskhalo — see what's eating your disk, as a sunburst or a treemap.

use diskhalo::{app, scan, sunburst, treemap, ui};

use anyhow::{Context, Result};
use app::{App, View};
use clap::Parser;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::execute;
use scan::Progress;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "diskhalo",
    version,
    about = "See what's eating your disk — as a sunburst or a treemap, in your terminal."
)]
struct Args {
    /// Directory to scan (defaults to the current directory).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Cross filesystem boundaries (off by default, so mounted volumes stay out).
    #[arg(short = 'x', long)]
    cross_filesystems: bool,

    /// Count hardlinked files once per link instead of once per inode.
    #[arg(long)]
    no_dedupe_links: bool,

    /// Start in the treemap view rather than the sunburst.
    #[arg(short = 't', long)]
    treemap: bool,

    /// Report apparent size instead of blocks actually allocated.
    #[arg(short = 'a', long)]
    apparent: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = args
        .path
        .canonicalize()
        .with_context(|| format!("cannot read {}", args.path.display()))?;
    if !root.is_dir() {
        anyhow::bail!("{} is not a directory", root.display());
    }

    let opts = scan::Opts {
        one_fs: !args.cross_filesystems,
        dedupe_links: !args.no_dedupe_links,
    };

    let mut term = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture).ok();

    let result = run(&mut term, root, opts, &args);

    execute!(io::stdout(), DisableMouseCapture).ok();
    ratatui::restore();
    result
}

fn run(
    term: &mut ratatui::DefaultTerminal,
    root: PathBuf,
    opts: scan::Opts,
    args: &Args,
) -> Result<()> {
    // Scan on a worker thread so the progress screen stays responsive and
    // Ctrl-C / q can abort a scan of somewhere enormous.
    let prog = Arc::new(Progress::default());
    let scan_prog = Arc::clone(&prog);
    let scan_root = root.clone();
    let handle = std::thread::spawn(move || scan::scan(&scan_root, opts, &scan_prog));

    while !prog.done.load(Ordering::Relaxed) {
        term.draw(|f| draw_scanning(f, &root, &prog))?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(k) = event::read()? {
                let abort = matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
                    || (k.code == KeyCode::Char('c')
                        && k.modifiers.contains(KeyModifiers::CONTROL));
                if abort && k.kind == KeyEventKind::Press {
                    prog.cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    let tree = handle
        .join()
        .map_err(|_| anyhow::anyhow!("scan thread panicked"))?
        .with_context(|| format!("scanning {}", root.display()))?;

    if prog.cancel.load(Ordering::Relaxed) {
        return Ok(());
    }

    let (_, _, _, denied) = prog.snapshot();
    let mut state = App::new(tree, root, denied);
    state.view = if args.treemap {
        View::Treemap
    } else {
        View::Sunburst
    };
    state.apparent = args.apparent;

    loop {
        // The list needs to know its own height before it can scroll sensibly.
        let size = term.size()?;
        let n = state.current().children.len();
        state.list_rows =
            ui::list_rows(ratatui::layout::Rect::new(0, 0, size.width, size.height), n);

        term.draw(|f| ui::draw(f, &state))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => handle_key(&mut state, k),
            Event::Mouse(m) => handle_mouse(&mut state, m, term.size()?),
            _ => {}
        }
        if state.quit {
            return Ok(());
        }
    }
}

fn handle_key(state: &mut App, k: event::KeyEvent) {
    // The confirmation modal swallows everything until it's answered.
    if let Some(idx) = state.confirm {
        match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                state.confirm = None;
                state.sel = idx;
                if let Err(e) = state.delete_selected() {
                    state.status = Some(format!("delete failed: {e}"));
                }
            }
            _ => {
                state.confirm = None;
                state.status = None;
            }
        }
        return;
    }

    match k.code {
        KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => state.quit = true,
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_sel(1);
            state.status = None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_sel(-1);
            state.status = None;
        }
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => state.enter(),
        KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => state.up(),
        KeyCode::Tab | KeyCode::Char('v') => {
            state.view = state.view.toggled();
            state.status = None;
        }
        KeyCode::Char('a') => {
            state.apparent = !state.apparent;
            state.status = Some(if state.apparent {
                "showing apparent size".into()
            } else {
                "showing size on disk".into()
            });
        }
        KeyCode::Char('g') => {
            state.sel = 0;
            state.list_top = 0;
        }
        KeyCode::Char('G') => {
            let n = state.current().children.len();
            state.sel = n.saturating_sub(1);
        }
        KeyCode::Char('d') | KeyCode::Delete if state.selected().is_some() => {
            state.confirm = Some(state.sel);
        }
        _ => {}
    }
    state.keep_selection_visible();
}

fn handle_mouse(state: &mut App, m: event::MouseEvent, size: ratatui::layout::Size) {
    if state.confirm.is_some() {
        return;
    }
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let viz = ui::viz_area(area);
    let list = ui::list_area(area);

    match m.kind {
        MouseEventKind::ScrollDown => state.move_sel(1),
        MouseEventKind::ScrollUp => state.move_sel(-1),
        MouseEventKind::Down(MouseButton::Left) => {
            if m.column >= list.x && m.column < list.x + list.width && m.row >= list.y {
                // Side list: row maps straight to a child index.
                let row = (m.row - list.y) as usize + state.list_top;
                if row < state.current().children.len() {
                    if state.sel == row {
                        state.enter();
                    } else {
                        state.sel = row;
                    }
                }
            } else if m.column >= viz.x
                && m.column < viz.x + viz.width
                && m.row >= viz.y
                && m.row < viz.y + viz.height
            {
                if let Some(i) = viz_hit(state, viz, m.column, m.row) {
                    if state.sel == i {
                        state.enter();
                    } else {
                        state.sel = i;
                    }
                }
            }
        }
        _ => {}
    }
    state.keep_selection_visible();
}

/// Which child of the current directory is under a screen cell.
fn viz_hit(state: &App, viz: ratatui::layout::Rect, col: u16, row: u16) -> Option<usize> {
    let node = state.current();
    if node.children.is_empty() {
        return None;
    }
    let lx = (col - viz.x) as f64;
    let ly = (row - viz.y) as f64;

    match state.view {
        View::Sunburst => {
            let rings = sunburst::build(
                node,
                state.apparent,
                ui::MAX_DEPTH,
                ui::radius_px(viz),
                ui::INNER_HOLE,
            );
            // Canvas pixels are one per column but two per row.
            sunburst::hit_test(
                viz.width as usize,
                viz.height as usize * 2,
                &rings,
                ui::INNER_HOLE,
                lx + 0.5,
                ly * 2.0 + 1.0,
            )
        }
        View::Treemap => {
            let weights: Vec<u64> = node
                .children
                .iter()
                .map(|c| c.size(state.apparent))
                .collect();
            let rects = treemap::layout_weights(
                &weights,
                treemap::Rect::new(0.0, 0.0, viz.width as f64, viz.height as f64),
            );
            rects.iter().position(|r| r.contains(lx, ly))
        }
    }
}

fn draw_scanning(f: &mut ratatui::Frame, root: &std::path::Path, prog: &Progress) {
    use ratatui::layout::Alignment;
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let area = f.area();
    f.buffer_mut().set_style(
        area,
        Style::default()
            .bg(Color::Rgb(14, 15, 20))
            .fg(Color::Rgb(214, 218, 230)),
    );
    let (dirs, files, bytes, denied) = prog.snapshot();

    // A small rotating ring, so it's visibly alive on a slow network volume.
    const SPIN: [&str; 8] = ["◜", "◠", "◝", "◞", "◡", "◟", "◜", "◠"];
    let tick = (dirs / 4) as usize % SPIN.len();

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  {}  ", SPIN[tick]),
                Style::default()
                    .fg(Color::Rgb(122, 196, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("scanning ", Style::default().fg(Color::Rgb(108, 116, 136))),
            Span::styled(
                app::elide(root, 48),
                Style::default()
                    .fg(Color::Rgb(122, 196, 255))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "     {} dirs · {} files · {}",
                dirs,
                files,
                app::human(bytes)
            ),
            Style::default().fg(Color::Rgb(214, 218, 230)),
        )),
    ];
    if denied > 0 {
        lines.push(Line::from(Span::styled(
            format!("     {denied} unreadable (grant Full Disk Access to include them)"),
            Style::default().fg(Color::Rgb(201, 142, 92)),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "     q to cancel",
        Style::default().fg(Color::Rgb(108, 116, 136)),
    )));

    let y = area.height / 2 - 3;
    let inner = ratatui::layout::Rect {
        x: area.x,
        y: area.y + y.min(area.height.saturating_sub(1)),
        width: area.width,
        height: lines.len().min(area.height as usize) as u16,
    };
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Left), inner);
}
