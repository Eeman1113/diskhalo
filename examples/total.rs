//! Print scan totals as text — used to cross-check the scanner against `du`.
//! usage: cargo run --example total -- <path>

use diskhalo::app::human;
use diskhalo::scan;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let path = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let prog = scan::Progress::default();
    let t = scan::scan(&path, scan::Opts::default(), &prog)?;
    let (dirs, files, _, denied) = prog.snapshot();
    println!("disk      {:>12} ({} bytes)", human(t.disk), t.disk);
    println!("apparent  {:>12} ({} bytes)", human(t.apparent), t.apparent);
    println!("dirs {dirs}  files {files}  unreadable {denied}");
    for c in t.children.iter().take(10) {
        println!(
            "  {:>10}  {}{}",
            human(c.disk),
            c.name,
            if c.is_dir { "/" } else { "" }
        );
    }
    Ok(())
}
