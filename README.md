<h1 align="center">diskhalo</h1>

<p align="center">
  <b>See what's eating your disk — as a sunburst or a treemap, in your terminal.</b>
</p>

<p align="center">
  <a href="https://github.com/Eeman1113/diskhalo/actions/workflows/ci.yml"><img src="https://github.com/Eeman1113/diskhalo/actions/workflows/ci.yml/badge.svg" alt="ci"></a>
  <a href="https://crates.io/crates/diskhalo"><img src="https://img.shields.io/crates/v/diskhalo.svg" alt="crates.io"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT"></a>
</p>

<p align="center">
  <img src="assets/demo.gif" alt="diskhalo scanning /opt/homebrew, switching between sunburst and treemap views" width="100%">
</p>

---

## Why

Every good terminal disk tool shows you a **list**. `ncdu`, `dust`, `gdu`, `dua` — all excellent, all fundamentally a sorted column of numbers. Lists are great at "what is the single biggest thing" and bad at "where did 40 GB actually go", because a list can't show you that two hundred medium directories outweigh the one big one at the top.

The tools that *do* answer that — DaisyDisk, WinDirStat, WizTree — are GUIs, and the two terminal programs that ever drew a real treemap (`diskonaut`, `squirreldisk`) are either unmaintained since 2022 or not a terminal program at all.

diskhalo draws both, in your terminal, over a scanner that is **byte-for-byte accurate** and **~5× faster than `du`**:

- **Sunburst** — concentric rings, one per directory level, arc length proportional to size. As far as I can tell this is the only one in a terminal.
- **Treemap** — squarified nested rectangles, so area maps to bytes.

Same scan, same keys, `tab` to switch.

## Install

```sh
# Homebrew
brew install Eeman1113/tap/diskhalo

# Cargo
cargo install diskhalo
```

Or grab a binary from [releases](https://github.com/Eeman1113/diskhalo/releases).

## Use

```sh
diskhalo                    # current directory
diskhalo ~/Library          # somewhere specific
diskhalo -t /               # start in the treemap view
diskhalo -a ~/Downloads     # report apparent size, not blocks on disk
```

| key | |
|---|---|
| <kbd>↑</kbd> <kbd>↓</kbd> / <kbd>k</kbd> <kbd>j</kbd> | select |
| <kbd>↵</kbd> / <kbd>→</kbd> | open the selected directory |
| <kbd>⌫</kbd> / <kbd>←</kbd> | go up |
| <kbd>tab</kbd> | switch sunburst ⇄ treemap |
| <kbd>a</kbd> | toggle apparent size vs size on disk |
| <kbd>d</kbd> | delete the selection (asks first) |
| <kbd>g</kbd> / <kbd>G</kbd> | jump to first / last |
| <kbd>q</kbd> | quit |

The mouse works too — click a wedge or rectangle to select it, click again to descend, scroll to move the selection.

```
-x, --cross-filesystems   descend into mounted volumes (off by default)
    --no-dedupe-links     count hardlinked files once per link, not once per inode
```

## How the sunburst works

A terminal cell is about twice as tall as it is wide, so anything circular drawn with whole cells comes out as an oval. diskhalo writes the **upper-half-block** glyph `▀` with one colour as foreground and another as background, which splits every cell into two stacked pixels — doubling vertical resolution and making pixels roughly square. The disc is rasterised into that buffer with 2×2 supersampling and alpha-blended against the background, which is what keeps the arcs smooth instead of staircased.

Three details did most of the work in making it readable:

- **Culling scales with radius.** A wedge spanning angle θ at radius r draws `θ·r` pixels of arc, so a flat angular cutoff either erases every outer ring or lets the inner ones dissolve into static. The threshold is computed per ring from its own radius.
- **Tiny siblings are aggregated, at every level.** Two hundred 1-pixel slivers are noise, not information. Anything too thin to see is folded into one desaturated wedge in its parent's hue, carried out to the rim. Doing this only at the top level leaves the outer rings — which own most of the disc's area — almost entirely black.
- **Rings are dropped when they stop carrying data.** An outer ring covering less than ~35% of the circle costs more area than it earns, so it's removed and the remaining rings get thicker.

The treemap is the standard [squarified](https://www.win.tue.nl/~vanwijk/stm.pdf) algorithm (Bruls, Huizing & van Wijk), which favours near-square rectangles so small entries stay clickable.

## Accuracy

Getting disk numbers right on macOS takes more care than walking the tree:

- **Blocks, not bytes.** The default is `blocks × 512` — what deleting something would actually give back. Sparse files and APFS clones report far more apparent bytes than they occupy, which is why Finder and `du` so often disagree. <kbd>a</kbd> shows the other number.
- **Hardlinks are counted once.** A file with `nlink > 1` is charged to the first link found and skipped after that, so a Time Machine-style tree doesn't report several times its real size.
- **APFS firmlinks are skipped.** `/System/Volumes/Data` is a firmlink back to the data volume *on the same device*, so a stay-on-one-filesystem check doesn't catch it and a naive scan of `/` silently double-counts the entire disk.
- **Symlinks are never followed**, so one loop can't eat the scan.
- Unreadable directories are counted and reported in the header rather than silently skipped. Most of those go away with Full Disk Access.

Verified against `du -sk` on 27.6 GB across 507,422 files: **identical to the byte.**

## Performance

Scanning `/opt/homebrew` — 65,434 directories, 507,422 files — on an Apple M4 (10 cores), warm cache, median of 5 runs:

| | time |
|---|---|
| `diskhalo` | **2.24 s** |
| `du -sk` | 12.33 s |

About 5.5× faster, because the walk is parallel (rayon, ~650–890% CPU) while `du` is single-threaded. The scan runs on a worker thread, so the progress screen stays live and <kbd>q</kbd> aborts a scan of somewhere enormous.

## Compared to

| | view | maintained |
|---|---|---|
| **diskhalo** | **sunburst + treemap** | ✅ |
| [`diskonaut`](https://github.com/imsnif/diskonaut) | treemap | last commit Feb 2022 |
| [`dust`](https://github.com/bootandy/dust) | list + bar chart | ✅ |
| [`gdu`](https://github.com/dundee/gdu), [`dua`](https://github.com/Byron/dua-cli), `ncdu` | list | ✅ |
| [DaisyDisk](https://daisydiskapp.com), WizTree | sunburst / treemap, GUI | ✅ (paid) |

They're all good tools — `dust` in particular is a better fit when you just want the top ten in a pipeline. diskhalo is for the case where you need to *see the shape* of the thing.

## Build from source

```sh
git clone https://github.com/Eeman1113/diskhalo
cd diskhalo
cargo build --release
cargo test
```

There's a dev harness that renders the TUI straight to an image, which is how the layout was debugged without squinting at a live terminal:

```sh
cargo run --release --example preview -- ~/Downloads out.ppm sunburst 150 44
cargo run --release --example preview -- synthetic out.ppm sunburst   # uniform tree: must be a perfect circle
cargo run --release --example total -- ~/Downloads                    # text totals, for cross-checking du
```

## Status

Works on macOS and Linux. Windows is not supported yet — the scanner reads `st_blocks` and `st_nlink` through `std::os::unix`, which needs a separate implementation there.

Known rough edges, contributions very welcome:

- No "group small items" control — the aggregation threshold is derived from the terminal size, not configurable.
- No search / filter.
- Deleting is immediate; there's no trash integration.

## License

MIT © [Eeman Majumder](https://github.com/Eeman1113)
