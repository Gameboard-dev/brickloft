# Development

## Build and test

```sh
cargo test                      # core, plugin and editor logic
cargo run -p arcs-editor        # the editor as a desktop window (program `arcs`)
cargo run -p arcs-editor -- 5   # the same, opening on the sixth form in the menu
sh build-editor.sh              # the editor for the browser, into dist/
cargo run --release -p arcs-core --example timing
```

## Inspecting a fit

```sh
cargo run -p arcs-core --example inspect -- arcs-report-1.txt
cargo run -p arcs-core --example inspect -- arcs-report-1.txt simplify=0 grain=1
```

The editor's **Save report** button writes `arcs-report-1.txt`, `-2` and so on
(never replacing an earlier one) where the desktop editor was started (in the browser it copies the report to the clipboard). A
report holds the form's settings, its bricks, and the bricks measured against
the form cell by cell: the cells one unit brick would improve, the worst
cells, and maps of the form, the bricks and their difference. `inspect`
rebuilds the form from a report's `[spec]` section and prints a fresh one;
extra `key=value` arguments override settings, to try a change on the same
form. `core/src/report.rs` documents the format.

`plugin/tests/fake_omegga.rs` starts the real plugin and plays omegga's and
the editor's parts, so the whole path up to the call into the game is tested
without a game. Each of its two tests has been seen to fail with its bug put
back (a forged token accepted; positions one unit off).

Nothing here has been run against a live server yet. See
[calibration.md](calibration.md) for what to check first.

## Dependencies

| Crate | Used by | Why this and not by hand |
|---|---|---|
| `glam` (f64 only) | core | vector maths |
| `serde_json` | plugin | omegga's JSON-RPC and the editor's requests |
| `tiny_http` | plugin | HTTP parsing is where hand-written servers go wrong |
| `getrandom` | plugin | link tokens must be unguessable; std has no secure RNG |
| `three-d` (window + egui only) | editor | rendering, orbit camera, picking, panel |
| `wasm-bindgen`, `web-sys`, `js-sys` | editor, browser only | the one request the page makes |

Not used: the `omegga` crate and `tokio` (the plugin is two blocking threads),
Bevy (several times the compile time and download for the same four needs),
any async runtime in the page.

`wasm-bindgen` is pinned to the version of the installed `wasm-bindgen`
command; the two must match exactly.

## Compile times (measured, Windows, cold cache)

| Step | Time |
|---|---|
| core + plugin, with tests | 11 s |
| editor dependencies, first time | 49 s |
| editor for the browser, after that | 7 s |
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

Sizes are in microbricks. The browser build is somewhat slower than these.
The two thin shells are over budget: see "Limits" in
[how-it-fits.md](how-it-fits.md).

## Where the time goes, and what was done about it

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
