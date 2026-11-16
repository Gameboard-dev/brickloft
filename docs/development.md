# Development

How to build, run, test and examine arcs. It is a Rust workspace of two
crates and needs only [Rust](https://rustup.rs).

## Run it

```sh
cargo run --release -p arcs-editor        # the editor (the program is called `arcs`)
cargo run --release -p arcs-editor -- 4   # the same, opening on the fifth form in the menu
```

Without `--release` it builds faster and the editor still runs well: the
fitting crate and all dependencies are optimised even in a debug build (see
`[profile.dev]` in `Cargo.toml`).

The program writes three kinds of file beside wherever it was started, so
start it from a folder you can write to:

| File | Written by |
|---|---|
| `arcs-build.brz` | Ctrl+C: the prefab put on the clipboard |
| `arcs-report-N.txt` | Save report |
| `arcs-forms/` | Saved forms |

All three are in `.gitignore`.

The optional browser page is built with `sh build-editor.sh`; see
[web-page.md](web-page.md).

## Test it

```sh
cargo test          # everything: about 110 tests, a few seconds
cargo test -p arcs-core
cargo test -p arcs-editor
```

What the tests hold the code to:

- **Shapes:** every pose of every brick has the right volume and corners,
  and every pose can be reached by one of the game's 24 orientations.
- **Fitters:** no two bricks hold the same point; every form builds bricks
  the game can place; spheres, rings and arches come out mirror symmetric; a
  pyramid is exactly its four slopes; named forms stay under a stated share
  of stepped faces.
- **Measurement:** a lone brick of each shape has the surface area of its
  solid, and the discrepancy the fitter reports is the real disagreement.
- **Editor:** dragging, undo, grids, picking, copy and paste, tested through
  the same functions the mouse and keys call, with no window.

A fix for a bug comes with a test that fails when the bug is put back.

## Examine a fit

A saved report can be rebuilt and measured again from the command line,
which is how a change to a fitter is judged.

```sh
# rebuild the form in a report and print a fresh report
cargo run --release -p arcs-core --example inspect -- arcs-report-1.txt

# the same form with settings changed
cargo run --release -p arcs-core --example inspect -- arcs-report-1.txt simplify=0 grain=1

# just the brick count, extent and discrepancy
cargo run --release -p arcs-core --example extent -- arcs-report-1.txt

# time a set of representative forms
cargo run --release -p arcs-core --example timing

# time the form in a report, measured and unmeasured, optionally scaled up
cargo run --release -p arcs-core --example timing -- arcs-report-1.txt 3
```

A report has these sections:

| Section | What it holds |
|---|---|
| `[spec]` | the form's settings, one `key = value` a line; this alone is enough to rebuild it |
| `[fit]` | brick counts by kind, the form's volume, the discrepancy |
| `[improvable]`, `[worst]` | the cells where bricks and form disagree most |
| `[surface]` | the share of the outside within 10, 20, 30 and 45 degrees of the true surface, the stepped share, the same by layer, the worst faces |
| `[section ...]` | cuts through bricks and form together, drawn in text |

In a section drawing, `#` is brick and form, `O` is brick outside the form,
`!` is form with no brick, and `.` is neither.

`core/src/report.rs` documents the format.

## The grid-at-an-angle trial

```sh
cargo run -p arcs-editor --example grid_trial
```

writes `arcs-grid-trial.brz`: a flat ring on the world's grid and an arch on
a second grid tilted 30° and 20°. It was written to find out whether the
game accepts a prefab with a tilted grid from the clipboard (it does).

## Layout of the code

```
core/src/
  curves.rs       curves, flattened to polylines
  sdf.rs          solids as distance functions
  shapes.rs       the nine brick shapes as corner masks
  orient.rs       corner mask -> the game's asset, orientation and size
  spec.rs         a form's settings -> the right fitter -> bricks
  fit_volume.rs   cell by cell
  fit_planar.rs   flat outlines
  fit_round.rs    spheres, ellipsoids, domes
  fit_pyramid.rs  pyramids
  fit_stack.rs    round tubes as pipes
  fit_lathe.rs    profiles spun about an axis
  fit_ribbon.rs   bands and tubes lifted out of their plane
  report.rs       measuring a fit; the report format
  surface.rs      the surface survey and section drawings
editor/src/
  lib.rs          the frame loop: events, fit, draw
  state.rs        the workspace: forms, dots, dragging, grids, undo
  panel.rs        the settings panel
  camera.rs       the flown camera
  brz.rs          the prefab file, and the clipboard
  mesh.rs         a triangle mesh for each brick shape
  material.rs     the bricks' shader, with edge lines
  paint.rs        colouring a form
  ruler.rs        the rulers
  store.rs        saved forms
  send.rs         saving a report
```

`core` depends on nothing but a vector-maths crate and knows nothing of the
game's file format or the window. Everything it does can be run and tested
from the command line.

## Dependencies

| Crate | Used by | Why this and not by hand |
|---|---|---|
| `glam` (f64 only) | core | vector maths |
| `three-d` (window and panel features only) | editor | rendering, picking and the panel in one crate |
| `brdb` (its `.brz` half only) | editor | the game's own prefab format; the alternative is re-deriving its schemas from the game with no way to check the result |
| `clipboard-win` | editor, Windows | putting a file on the clipboard; the alternative is the raw Win32 calls it wraps |
| `wasm-bindgen`, `web-sys`, `js-sys` | editor, browser page only | the bridge between the compiled program and the page |

Not used: a game engine (several times the compile time and download for
the same needs), or any async runtime.

## Conventions

- Comments say why a thing is done the way it is, and what was tried
  before, not what the line does.
- Where a method was replaced, the comment keeps the reason and the
  measurement that decided it.
- Long lines are deliberate; the code is not run through `cargo fmt`.
