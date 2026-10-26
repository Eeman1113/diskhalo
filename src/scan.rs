//! Parallel filesystem scanner.
//!
//! Walks a directory tree with rayon, aggregating both apparent size (what the
//! file claims) and on-disk size (blocks actually allocated). The two diverge a
//! lot on APFS: sparse files and clones report far more apparent bytes than they
//! occupy, which is why `du` and Finder habitually disagree.

use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Live counters the UI polls while a scan is in flight.
#[derive(Default)]
pub struct Progress {
    pub dirs: AtomicU64,
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub denied: AtomicU64,
    pub done: AtomicBool,
    pub cancel: AtomicBool,
}

impl Progress {
    /// (dirs, files, bytes, denied)
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.dirs.load(Ordering::Relaxed),
            self.files.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            self.denied.load(Ordering::Relaxed),
        )
    }
}

/// One node in the scanned tree. Files have no children.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    /// Logical size as reported by the filesystem.
    pub apparent: u64,
    /// Allocated blocks * 512 — what freeing this would actually reclaim.
    pub disk: u64,
    /// Number of regular files at or below this node.
    pub files: u64,
    pub is_dir: bool,
    /// We could not read this directory (no permission).
    pub denied: bool,
    pub children: Vec<Entry>,
}

impl Entry {
    pub fn size(&self, apparent: bool) -> u64 {
        if apparent {
            self.apparent
        } else {
            self.disk
        }
    }

    fn leaf(name: String, apparent: u64, disk: u64) -> Self {
        Entry {
            name,
            apparent,
            disk,
            files: 1,
            is_dir: false,
            denied: false,
            children: Vec::new(),
        }
    }

    fn dir(name: String) -> Self {
        Entry {
            name,
            apparent: 0,
            disk: 0,
            files: 0,
            is_dir: true,
            denied: false,
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Opts {
    /// Stay on the filesystem the root lives on.
    pub one_fs: bool,
    /// Count each inode once, even when hardlinked many times.
    pub dedupe_links: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            one_fs: true,
            dedupe_links: true,
        }
    }
}

struct Ctx<'a> {
    prog: &'a Progress,
    opts: Opts,
    root_dev: u64,
    /// Inodes of multiply-linked files we have already charged.
    seen_links: Mutex<HashSet<u64>>,
}

/// Paths that would make a scan of `/` lie.
///
/// On APFS, `/System/Volumes/Data` is a *firmlink* back to the data volume — same
/// device, so a `one_fs` check won't catch it, and walking it double-counts the
/// entire disk. The sibling volumes are likewise bookkeeping, not user data.
fn is_macos_shadow(path: &Path) -> bool {
    let p = path.as_os_str().as_encoded_bytes();
    p == b"/System/Volumes" || p.starts_with(b"/System/Volumes/") || p == b"/.vol" || p == b"/dev"
}

fn scan_dir(path: &Path, ctx: &Ctx) -> Entry {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let mut node = Entry::dir(name);

    if ctx.prog.cancel.load(Ordering::Relaxed) {
        return node;
    }

    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            ctx.prog.denied.fetch_add(1, Ordering::Relaxed);
            node.denied = true;
            return node;
        }
    };
    ctx.prog.dirs.fetch_add(1, Ordering::Relaxed);

    // Split into files (charged inline) and subdirectories (recursed in parallel).
    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut files: Vec<Entry> = Vec::new();

    for dent in rd.flatten() {
        let child = dent.path();
        // symlink_metadata: never follow links, or a single loop eats the scan.
        let md = match fs::symlink_metadata(&child) {
            Ok(md) => md,
            Err(_) => continue,
        };
        let ft = md.file_type();

        if ft.is_dir() {
            if ctx.opts.one_fs && md.dev() != ctx.root_dev {
                continue;
            }
            if is_macos_shadow(&child) {
                continue;
            }
            subdirs.push(child);
        } else if ft.is_file() {
            // A hardlinked file occupies its blocks once; charging every link
            // inflates the total. Only multiply-linked files need the set, so
            // lock contention stays near zero on normal trees.
            if ctx.opts.dedupe_links && md.nlink() > 1 {
                let mut seen = ctx.seen_links.lock().unwrap();
                if !seen.insert(md.ino()) {
                    continue;
                }
            }
            let apparent = md.len();
            let disk = md.blocks().saturating_mul(512);
            ctx.prog.files.fetch_add(1, Ordering::Relaxed);
            ctx.prog.bytes.fetch_add(disk, Ordering::Relaxed);
            files.push(Entry::leaf(
                dent.file_name().to_string_lossy().into_owned(),
                apparent,
                disk,
            ));
        }
        // Symlinks themselves are a few bytes of metadata; ignore them.
    }

    let mut kids: Vec<Entry> = subdirs.par_iter().map(|p| scan_dir(p, ctx)).collect();
    kids.append(&mut files);

    for k in &kids {
        node.apparent += k.apparent;
        node.disk += k.disk;
        node.files += k.files;
    }
    node.children = kids;
    node
}

/// Sort every level biggest-first so layout and list agree on ordering.
fn sort_tree(node: &mut Entry) {
    node.children
        .sort_unstable_by(|a, b| b.disk.cmp(&a.disk).then_with(|| a.name.cmp(&b.name)));
    for c in &mut node.children {
        sort_tree(c);
    }
}

/// Scan `root`, reporting progress into `prog` as it goes.
pub fn scan(root: &Path, opts: Opts, prog: &Progress) -> std::io::Result<Entry> {
    let md = fs::metadata(root)?;
    let ctx = Ctx {
        prog,
        opts,
        root_dev: md.dev(),
        seen_links: Mutex::new(HashSet::new()),
    };

    let mut tree = scan_dir(root, &ctx);
    tree.name = root.to_string_lossy().into_owned();
    sort_tree(&mut tree);
    prog.done.store(true, Ordering::Relaxed);
    Ok(tree)
}
