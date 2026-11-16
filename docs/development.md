# Development

## Build and test

```sh
cargo test                      # core and editor logic
cargo run -p arcs-editor        # the editor as a desktop window (program `arcs`)
cargo run -p arcs-editor -- 5   # the same, opening on the sixth form in the menu
cargo run --release -p arcs-core --example timing
```

## Inspecting a fit

```sh
cargo run -p arcs-core --example inspect -- arcs-report-1.txt
cargo run -p arcs-core --example inspect -- arcs-report-1.txt simplify=0 grain=1
```

The editor's **Save report** button writes `arcs-report-1.txt`, `-2` and so on
(never replacing an earlier one) where the editor was started. A
report holds the form's settings, its bricks, and the bricks measured against
the form cell by cell: the cells one unit brick would improve, the worst
cells, and maps of the form, the bricks and their difference. `inspect`
rebuilds the form from a report's `[spec]` section and prints a fresh one;
extra `key=value` arguments override settings, to try a change on the same
form. `core/src/report.rs` documents the format.

The brick orientation rules have been confirmed in the game, by pasting
forms from the clipboard.

## Dependencies

| Crate | Used by | Why this and not by hand |
|---|---|---|
| `glam` (f64 only) | core | vector maths |
| `three-d` (window + egui only) | editor | rendering, camera, picking, panel |
| `brdb` (its .brz half only) | editor | the game's own prefab file, for the clipboard |
| `clipboard-win` | editor, Windows | putting a file on the clipboard |

Not used: Bevy (several times the compile time and download for the same
four needs).

## Compile times (measured, Windows, cold cache)

| Step | Time |
|---|---|
| core, with tests | 11 s |
| editor dependencies, first time | 49 s |
| core, after an edit | 3 to 5 s |

Dependencies are built optimised once and cached; the workspace's own crates
build unoptimised with thin debug info (`Cargo.toml`, `[profile.dev]`).

## Fit times (measured, release build)

Closest fit (Simplify 0, brick size 1):

| Form | Time | Bricks | Discrepancy |
|---|---|---|---|
| Round arch, span 32, 3 thick, 4 deep | 4 ms | 40 | 4.9% |
| Round arch, span 160 | 154 ms | 149 | 3.9% |
| Circle band, radius 60 | 139 ms | 181 | 3.8% |
| Solid sphere, radius 12 | 7 ms | 217 | 4.0% |
| Solid sphere, radius 50 | 128 ms | 2,215 | 1.2% |
| Hollow sphere, radius 50, wall 2 | 263 ms | 35,191 | 9.8% |
| Hollow dome, radius 100, wall 2 | 663 ms | 71,041 | 9.5% |
| Torus, radius 40, 8 thick | 149 ms | 573 | 12.0% |

The editor's starting balance (Simplify 0.25, automatic size, budget 2500):

| Form | Time | Bricks | Brick size | Discrepancy |
|---|---|---|---|---|
| Round arch, span 32 | 3 ms | 32 | 1 | 7.3% |
| Round arch, span 160 | 162 ms | 112 | 1 | 3.9% |
| Circle band, radius 60 | 149 ms | 149 | 1 | 4.6% |
| Solid sphere, radius 12 | 9 ms | 217 | 1 | 4.0% |
| Solid sphere, radius 50 | 164 ms | 2,215 | 1 | 1.2% |
| Hollow sphere, radius 50, wall 2 | 237 ms | 35,191 | 1 | 9.8% |
| Hollow dome, radius 100, wall 2 | 641 ms | 71,041 | 1 | 9.5% |
| Torus, radius 40, 8 thick | 201 ms | 573 | 1 | 12.0% |

Spheres and tori are laid out whole (see how-it-fits.md); the two large thin
shells are past what that method's lattice allows and are still fitted cell
by cell.

Simplify thins flat forms only; volumes are thinned by brick size.

Sizes are in microbricks.
The two thin shells are over budget: see "Limits" in
[how-it-fits.md](how-it-fits.md).

## Where the time goes, and what was done about it

Measured again after the changes below (release build, brick size 1), as
the editor builds while a form is being dragged, and in full:

| Form | Dragging | Let go (measured) |
|---|---|---|
| Flat arch, span 320 | 156 ms | 156 ms (was 1,250 ms) |
| Arch as a pipe, span 320 | 33 ms | 127 ms |
| Ring as a pipe, radius 80 | 41 ms | 175 ms |
| Solid sphere, radius 80 | 7 ms | 295 ms (was 51,000 bricks, now 3,400) |

- **Triangles are measured a sample row at a time**, from running counts
  along each row, not a cell at a time across their whole box. On a large
  flat arch that was nine tenths of the time.
- **A form laid out whole is measured only once the mouse is let go.**
  Measuring a sphere or a pipe against its form takes several times as long
  as building it, and while dragging only the bricks are wanted.
- **Measuring has no record to keep**, so its limit is the build's own. The
  report's 16 bytes a cell had capped it at 4 million cells, and a sphere
  over 78 in radius was fitted cell by cell for want of a measurement.

Still slow: a thin hollow sphere above about radius 45 is fitted cell by
cell (see how-it-fits.md), half a second at radius 80 and several at 160.

- **Volume fitter:** the form is tested once per 8 × 8 × 8 block of cells;
  blocks wholly inside or outside are never sampled. Each remaining cell's 64
  samples are one `u64`, compared with each candidate shape by XOR and a
  population count.
- **Planar fitter:** a candidate triangle is compared with the form a row of
  8 samples at a time, and with the unit pieces it would replace through a
  summed-area table (four lookups whatever its size).
- **Distance to a curve:** segments are filed in a bucket grid, so a query
  reads one bucket, not the whole curve.
- **Drawing:** one instanced mesh per brick shape, so a build is at most 29
  draw calls however many bricks it has.

The torus was the slowest form per brick (313 ms) while it was built as a
small disc spun about an axis, which tests every sample against every edge
of the disc. As its own formula it takes 21 ms.

- **Stretched ramps in 3D:** each surface cell is tried with the one ramp
  that faces the way the form does there, found from which half of the cell
  holds more of it, and cells are looked up through a direct table. The first
  version tried all twelve ramps at every cell and searched a sorted list for
  each lookup: 3.1 s for a radius-50 sphere against 0.15 s now.
- **Shadows:** the shadow map is redrawn only when the bricks change, not
  every frame.
- **Edge lines:** drawn in the bricks' own shader from per-vertex edge
  distances, so they add no geometry and no draw calls.
