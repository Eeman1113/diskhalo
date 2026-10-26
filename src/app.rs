//! Application state and navigation.

use crate::scan::Entry;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Sunburst,
    Treemap,
}

impl View {
    pub fn toggled(self) -> Self {
        match self {
            View::Sunburst => View::Treemap,
            View::Treemap => View::Sunburst,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            View::Sunburst => "sunburst",
            View::Treemap => "treemap",
        }
    }
}

pub struct App {
    pub root: Entry,
    pub root_path: PathBuf,
    /// Child indices from the root down to the directory being displayed.
    pub path: Vec<usize>,
    /// Selected child of the current directory.
    pub sel: usize,
    pub view: View,
    /// Show apparent size rather than blocks actually allocated.
    pub apparent: bool,
    pub quit: bool,
    /// Pending delete confirmation, holding the child index.
    pub confirm: Option<usize>,
    pub status: Option<String>,
    pub denied: u64,
    /// Scroll offset of the side list.
    pub list_top: usize,
    /// Visible rows in the side list, refreshed from the terminal each frame so
    /// scrolling knows how far it can see.
    pub list_rows: usize,
}

impl App {
    pub fn new(root: Entry, root_path: PathBuf, denied: u64) -> Self {
        App {
            root,
            root_path,
            path: Vec::new(),
            sel: 0,
            view: View::Sunburst,
            apparent: false,
            quit: false,
            confirm: None,
            status: None,
            denied,
            list_top: 0,
            list_rows: 1,
        }
    }

    pub fn current(&self) -> &Entry {
        let mut node = &self.root;
        for &i in &self.path {
            match node.children.get(i) {
                Some(c) => node = c,
                None => break,
            }
        }
        node
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.current().children.get(self.sel)
    }

    /// Filesystem path of the current directory.
    pub fn current_path(&self) -> PathBuf {
        let mut p = self.root_path.clone();
        let mut node = &self.root;
        for &i in &self.path {
            match node.children.get(i) {
                Some(c) => {
                    p.push(&c.name);
                    node = c;
                }
                None => break,
            }
        }
        p
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        let s = self.selected()?;
        let mut p = self.current_path();
        p.push(&s.name);
        Some(p)
    }

    pub fn move_sel(&mut self, delta: isize) {
        let n = self.current().children.len();
        if n == 0 {
            return;
        }
        let cur = self.sel as isize;
        self.sel = (cur + delta).rem_euclid(n as isize) as usize;
    }

    /// Scroll the side list so the selected row is on screen.
    pub fn keep_selection_visible(&mut self) {
        let rows = self.list_rows.max(1);
        let n = self.current().children.len();
        if n == 0 {
            self.list_top = 0;
            return;
        }
        if self.sel < self.list_top {
            self.list_top = self.sel;
        } else if self.sel >= self.list_top + rows {
            self.list_top = self.sel + 1 - rows;
        }
        // Never leave blank rows at the bottom when the list can fill them.
        self.list_top = self.list_top.min(n.saturating_sub(rows));
    }

    pub fn enter(&mut self) {
        let is_dir = self
            .selected()
            .map(|s| s.is_dir && !s.children.is_empty())
            .unwrap_or(false);
        if is_dir {
            self.path.push(self.sel);
            self.sel = 0;
            self.list_top = 0;
            self.status = None;
        }
    }

    pub fn up(&mut self) {
        if let Some(i) = self.path.pop() {
            self.sel = i;
            self.list_top = 0;
            self.status = None;
        }
    }

    /// Delete the selected entry from disk, then from the in-memory tree.
    ///
    /// Sizes are subtracted from every ancestor so the visualisation stays
    /// truthful without the cost of a full rescan.
    pub fn delete_selected(&mut self) -> std::io::Result<()> {
        let Some(target) = self.selected_path() else {
            return Ok(());
        };
        let Some(entry) = self.selected() else {
            return Ok(());
        };
        let (is_dir, apparent, disk, files) =
            (entry.is_dir, entry.apparent, entry.disk, entry.files);
        let name = entry.name.clone();

        if is_dir {
            std::fs::remove_dir_all(&target)?;
        } else {
            std::fs::remove_file(&target)?;
        }

        // Walk down the stored path, debiting each ancestor on the way.
        let idx = self.sel;
        let path = self.path.clone();
        let mut node = &mut self.root;
        node.apparent -= apparent.min(node.apparent);
        node.disk -= disk.min(node.disk);
        node.files -= files.min(node.files);
        for &i in &path {
            node = &mut node.children[i];
            node.apparent -= apparent.min(node.apparent);
            node.disk -= disk.min(node.disk);
            node.files -= files.min(node.files);
        }
        if idx < node.children.len() {
            node.children.remove(idx);
        }
        let n = node.children.len();
        if n == 0 {
            self.sel = 0;
        } else if self.sel >= n {
            self.sel = n - 1;
        }
        self.status = Some(format!("deleted {name} ({})", human(disk)));
        Ok(())
    }
}

/// Format bytes for display. Base 1024, short suffixes.
pub fn human(b: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    if b < 1024 {
        return format!("{b} B");
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{:.0} {}", v, UNITS[i])
    } else if v >= 10.0 {
        format!("{:.1} {}", v, UNITS[i])
    } else {
        format!("{:.2} {}", v, UNITS[i])
    }
}

/// Shorten a long path for the header, keeping the tail which is what matters.
pub fn elide(p: &Path, max: usize) -> String {
    let s = p.to_string_lossy();
    if s.chars().count() <= max {
        return s.into_owned();
    }
    let tail: String = s
        .chars()
        .rev()
        .take(max.saturating_sub(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Entry {
        let f = |n: &str, s: u64| Entry {
            name: n.into(),
            apparent: s,
            disk: s,
            files: 1,
            is_dir: false,
            denied: false,
            children: vec![],
        };
        Entry {
            name: "/root".into(),
            apparent: 300,
            disk: 300,
            files: 3,
            is_dir: true,
            denied: false,
            children: vec![
                Entry {
                    name: "big".into(),
                    apparent: 200,
                    disk: 200,
                    files: 2,
                    is_dir: true,
                    denied: false,
                    children: vec![f("x", 150), f("y", 50)],
                },
                f("lone", 100),
            ],
        }
    }

    #[test]
    fn navigation_tracks_paths() {
        let mut app = App::new(tree(), PathBuf::from("/root"), 0);
        assert_eq!(app.current_path(), PathBuf::from("/root"));
        app.enter();
        assert_eq!(app.current_path(), PathBuf::from("/root/big"));
        assert_eq!(app.current().children.len(), 2);
        app.up();
        assert_eq!(app.current_path(), PathBuf::from("/root"));
        assert_eq!(
            app.sel, 0,
            "returning should restore the child we came from"
        );
    }

    #[test]
    fn enter_does_nothing_on_a_file() {
        let mut app = App::new(tree(), PathBuf::from("/root"), 0);
        app.sel = 1; // "lone", a file
        app.enter();
        assert_eq!(
            app.current_path(),
            PathBuf::from("/root"),
            "must not descend into a file"
        );
    }

    #[test]
    fn selection_wraps_both_ways() {
        let mut app = App::new(tree(), PathBuf::from("/root"), 0);
        app.move_sel(-1);
        assert_eq!(app.sel, 1, "up from the first child wraps to the last");
        app.move_sel(1);
        assert_eq!(app.sel, 0);
    }

    #[test]
    fn up_at_root_is_a_noop() {
        let mut app = App::new(tree(), PathBuf::from("/root"), 0);
        app.up();
        assert_eq!(app.current_path(), PathBuf::from("/root"));
    }

    fn wide(n: usize) -> Entry {
        let kids: Vec<Entry> = (0..n)
            .map(|i| Entry {
                name: format!("f{i}"),
                apparent: (n - i) as u64,
                disk: (n - i) as u64,
                files: 1,
                is_dir: false,
                denied: false,
                children: vec![],
            })
            .collect();
        Entry {
            name: "/w".into(),
            apparent: 1,
            disk: 1,
            files: n as u64,
            is_dir: true,
            denied: false,
            children: kids,
        }
    }

    #[test]
    fn list_scrolls_down_to_follow_selection() {
        let mut app = App::new(wide(100), PathBuf::from("/w"), 0);
        app.list_rows = 10;
        for _ in 0..20 {
            app.move_sel(1);
            app.keep_selection_visible();
        }
        assert_eq!(app.sel, 20);
        assert!(
            app.sel >= app.list_top && app.sel < app.list_top + 10,
            "sel {} outside window starting {}",
            app.sel,
            app.list_top
        );
    }

    #[test]
    fn list_scrolls_back_up() {
        let mut app = App::new(wide(100), PathBuf::from("/w"), 0);
        app.list_rows = 10;
        app.sel = 50;
        app.keep_selection_visible();
        app.sel = 3;
        app.keep_selection_visible();
        assert_eq!(app.list_top, 3);
    }

    #[test]
    fn wrapping_to_the_end_scrolls_to_the_end() {
        let mut app = App::new(wide(100), PathBuf::from("/w"), 0);
        app.list_rows = 10;
        app.move_sel(-1); // wrap to last
        app.keep_selection_visible();
        assert_eq!(app.sel, 99);
        assert_eq!(
            app.list_top, 90,
            "last page should be flush with the bottom"
        );
    }

    #[test]
    fn short_list_never_scrolls() {
        let mut app = App::new(tree(), PathBuf::from("/root"), 0);
        app.list_rows = 40;
        app.move_sel(-1);
        app.keep_selection_visible();
        assert_eq!(app.list_top, 0);
    }

    #[test]
    fn human_sizes_read_sensibly() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1024), "1.00 K");
        assert_eq!(human(1536), "1.50 K");
        assert_eq!(human(10 * 1024), "10.0 K");
        assert_eq!(human(1024 * 1024), "1.00 M");
    }
}
