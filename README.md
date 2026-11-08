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
cargo run --release -p azam-app -- shirt.azam.json   # the window, on a saved project
cargo run --release -p azam-app -- --screenshot shot.png --workspace grading --template labadeh
cargo run --release -p azam-app -- --screenshot drape.png --template dress --frames 300   # simulate first
cargo run --release -p azam-app -- --render render.png --template dress --frames 300      # path-traced
```

On macOS, `scripts/bundle-macos.sh` builds `target/release/azam.app` (with the MakeHuman pack inside).

`--workspace` is one of `design`, `grading`, `colorway`, `review`, `techpack`; `--template` one of
`tee`, `dress`, `skirt`, `labadeh`, `aba`.

Blender's controls apply: middle drag orbits the 3D view (Shift pans), the wheel zooms, `Z` opens the
shading pie, `` ` `` the view pie, `F3` the operator search, `Space` plays the simulation, `Ctrl Z`
undoes. Right-click opens a context menu for what is under the pointer (a piece, empty pattern space,
the 3D view); the editor-type button at the left of every area header switches the editor it shows
(choosing one another area shows swaps them); F3 ranks matches and shows each command's menu and
shortcut. The editor-type menu also splits any area side by side or one over the other (as often as you like, as in
Blender) or joins it with its neighbour; every border between areas can be dragged, and a narrow 3D
view's header leaves out its less used controls. Right-click on a seam or an outline point for their
own menus; File ▸ Import Avatar OBJ… dresses a body mesh of your own (scaled to the avatar's height). N over the 3D view
opens its sidebar (the selected piece's placement as numbers); over the pattern editor it is the Sew
tool, as each Blender editor has its own keys. With the Select tool and pieces selected, G, R and S with
the pointer over them move, turn and scale them modally (X or Y locks the axis; click or Enter keeps,
Esc or right-click cancels and leaves nothing to redo); elsewhere these keys pick their tools. The 3D
header's mode selector switches between Garment Mode and Arrange Mode (arrangement points shown).
Drags and modal transforms are one undo step each; separate clicks are always separate steps.
After a dart or a rectangle, an Adjust Last Operation panel changes its size. Pattern ▸ View ▸ Tool Names in Shelf
shows the tool shelves with labels. The pies are also under View ▸ Pie Menus. In the pattern editor: `W` select, `E` edit points,
`P` add point, `H` polygon, `S` rectangle, `D` dart, `N` sew (`Shift N` free sew), `M` measure; hover a
tool for its name and key. The Dart tool cuts a 20 × 80 mm dart into the clicked segment and sews its
legs; reshape it with Edit Points.

A piece's starting place around the avatar (slot, distance, angle, height) is in the Piece tab's
Arrangement panel; imported DXF pieces are placed by their names ("Back", "Sleeve L", "Collar"…).
Sizes are graded in the Project tab (size run, base size) and the Piece tab's Grading panel (a step
per size for the selected points). View ▸ Fit Map (or the 3D header in fit map shading) switches
between the Fit, Strain, Stress and Pressure maps. Graphics placed on pieces are printed on the cloth
in the 3D view and in renders (F12), together with topstitches, buttons, buttonholes and zippers
(the Zipper tool sews two edges and gives each a zipper half; in the simulation the cloth under its tape barely stretches or bends, and the zipper's weight hangs on the edge). Pins set with the 3D Pin tool are saved
with the project. In the 3D view, drag a piece to move it before draping, Ctrl-drag to turn it around
the body (saved with the project and undoable), or show the arrangement points (header button or View menu) and click one to send the
selected pieces there. The Dart tool's width and depth sit in the pattern editor's header while it
is active. Fabrics can be deleted, saved to a fabric library shared by all projects
(`~/Library/Application Support/azam/fabrics` on macOS, or `$AZAM_LIBRARY/fabrics`) and added from
it; DXF import also brings notches and grain lines, and File ▸ Import SVG… makes pieces of a drawing's
closed shapes (curves kept).

## Layout

| Path | What it is |
| --- | --- |
| `crates/azam-geom` | Pattern geometry: outlines with Bézier curve points, seam and hem allowance, sewing sides, grading. |
| `crates/azam-mesh` | Triangulating pieces for simulation: boundary sampling, grain lattice, Delaunay, topology, point location. |
| `crates/azam-doc` | The project document: pieces, seams, fabrics, colorways, templates, undoable edit commands, file format. |
| `crates/azam-avatar` | The avatar: MakeHuman morph body fitted to measurements (default), parametric SDF mannequin, collision grid, arrangement slots, posing and motions (raise arms, walk, sit, turntable). |
| `crates/azam-sim` | The XPBD cloth solver on the CPU: constraints, sewing, body and self collision, untangling, fit maps. |
| `crates/azam-sim-gpu` | The same solver as wgpu compute shaders (23 passes), sharing the renderer's device; the app falls back to the CPU solver on any GPU error. |
| `crates/azam-io` | DXF import (outlines, internal lines, notches, grain) and export; SVG import and export; OBJ export; tech pack tables; pattern and tech-pack PDF with embedded fonts (shaped Persian/Arabic, Hebrew, Greek, and a bundled CJK subset). |
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
- Native controls that stick out of their scroll container hide instead of painting over what lies
  beyond it (the kit cannot cut them partially).

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
