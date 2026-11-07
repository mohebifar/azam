# azam

A native garment design app — 2D pattern drafting, sewing, a fitted avatar and cloth simulation —
with Blender's interface, built on a fork of [BUI](https://github.com/ppmpreetham/BUI).
Licensed GPL-3.0-or-later (see `LICENSE`).

Plan and design notes: [azam: Garment Design App Plan](https://claude.ai/artifact/HY8Ws8ENmeHkthRMjgy2Sa).

## Running

After cloning, fetch the BUI fork once (upstream BUI at a pinned commit plus azam's patch):

```bash
scripts/setup-bui.sh
```

Then:

```bash
cargo run --release -p azam-app                      # the window
cargo run --release -p azam-app -- --screenshot shot.png --workspace grading --template labadeh
cargo run --release -p azam-app -- --screenshot drape.png --template dress --frames 300   # simulate first
cargo run --release -p azam-app -- --render render.png --template dress --frames 300      # path-traced
```

On macOS, `scripts/bundle-macos.sh` builds `target/release/azam.app` (with the MakeHuman pack inside).

`--workspace` is one of `design`, `grading`, `colorway`, `review`, `techpack`; `--template` one of
`tee`, `dress`, `skirt`, `labadeh`, `aba`.

Blender's controls apply: middle drag orbits the 3D view (Shift pans), the wheel zooms, `Z` opens the
shading pie, `` ` `` the view pie, `F3` the operator search, `Space` plays the simulation, `Ctrl Z`
undoes. In the pattern editor: `W` select, `E` edit points, `P` add point, `S` sew, `M` measure.

## Layout

| Path | What it is |
| --- | --- |
| `crates/azam-geom` | Pattern geometry: outlines with Bézier curve points, seam and hem allowance, sewing sides, grading. |
| `crates/azam-mesh` | Triangulating pieces for simulation: boundary sampling, grain lattice, Delaunay, topology, point location. |
| `crates/azam-doc` | The project document: pieces, seams, fabrics, colorways, templates, undoable edit commands, file format. |
| `crates/azam-avatar` | The avatar: MakeHuman morph body fitted to measurements (default), parametric SDF mannequin, collision grid, arrangement slots, posing and motions (raise arms, walk, sit, turntable). |
| `crates/azam-sim` | The XPBD cloth solver on the CPU: constraints, sewing, body and self collision, untangling, fit maps. |
| `crates/azam-sim-gpu` | The same solver as wgpu compute shaders (23 passes), sharing the renderer's device; the app falls back to the CPU solver on any GPU error. |
| `crates/azam-io` | DXF/SVG/OBJ import and export, tech pack tables, pattern and tech-pack PDF (embedded, shaped Persian/Arabic, Hebrew and Greek text; CJK through a system font when one is installed). |
| `crates/azam-trace` | Path tracer for presentation renders (Render ▸ Render Image, F12): BVH, cloth BSDF with sheen, studio lights, shadow-catcher floor, denoiser. |
| `crates/azam-sync` | Live collaboration: a yrs CRDT of the project, peer-to-peer over TCP with mDNS discovery, or an update log next to the project file (File ▸ Share / Join Session). |
| `crates/azam-render` | Offscreen wgpu renderer for the 3D viewport: studio lighting, key-light shadow map with a shadow-catching floor, procedural fabric weaves, Blender's floor grid. |
| `crates/azam-app` | The application: Blender-style window, editors, operators. Binary `azam`. |
| `tools/mhpack` | Builds `assets/makehuman/makehuman-cc0.bin` from MakeHuman's CC0 assets (see `assets/makehuman/README.md`). |
| `third_party/bui` | The BUI fork, created by `scripts/setup-bui.sh` (git-ignored): upstream BUI at `third_party/bui-base-commit.txt` plus `third_party/bui-azam.patch`. After changing the fork, regenerate the patch with `cd third_party/bui && git add -N . && git diff --binary > ../bui-azam.patch`. |
| `tools/golden` | Node scripts that run the reference JavaScript engine (`src.js`) and record golden data for the Rust tests. |
| `src.js` | The original JavaScript app, kept as the reference implementation. |

### Changes in the BUI fork

- `BK_icon_imbuf` / `Image::icon` / `icon_names`: Blender's SVG icons as images, for toolbars and menus.
- `label` attribute on number fields and sliders: Blender's "Label: value" fields.
- `autofocus` and `live` attributes on text fields: typing goes straight into the operator search,
  and every keystroke is reported.
- `file_dialog`: Blender's File Browser (a native modal window) from Rust, for Open, Save and
  exports.
- Persian and Arabic text: contextual shaping (joined letter forms, lam-alef, ZWNJ) before bidi
  reordering, and per-glyph fallback to a system font for scripts the UI font lacks
  (`BUI_FALLBACK_FONT` overrides the font file).

## Working on it

```bash
cargo test --workspace                       # unit, golden and headless UI tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt-azam                               # formats azam only (not the vendored BUI)
```

Building needs CMake and a C compiler (BUI's kit is C). Golden data is regenerated with the scripts
in `tools/golden` (run `npm install` there first), for example:

```bash
node tools/golden/export-geom.mjs > crates/azam-geom/tests/golden/geom.json
```
