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
cargo run --release -p azam-app -- --screenshot r.png --template dress --frames 300 --shading rendered
```

On macOS, `scripts/bundle-macos.sh` builds `target/release/azam.app` (with the MakeHuman pack inside).

`--workspace` is one of `design`, `grading`, `colorway`, `review`, `techpack`; `--template` one of
`tee`, `dress`, `skirt`, `labadeh`, `aba`, `blank`; `azam --help` lists every option.

Blender's controls apply: middle drag orbits the 3D view (Shift pans), the wheel zooms, `Z` opens the
shading pie, `` ` `` the view pie, `F3` the operator search, `Space` plays the simulation, `Ctrl Z`
undoes, `Ctrl Space` maximizes the area under the pointer (again: all areas back). F3 lists the
commands used last first, highlights the one Enter runs and says when nothing matches. The app auto
saves a copy every two minutes to its data folder (File ▸ Recover Auto Save opens it; Preferences
sets the interval or turns it off); File ▸ New ▸ Blank starts an empty board. Right-click opens a context menu for what is under the pointer (a piece, empty pattern space,
the 3D view); the editor-type button at the left of every area header switches the editor it shows
(choosing one another area shows swaps them); F3 ranks matches and shows each command's menu and
shortcut. The editor-type menu also splits any area side by side or one over the other (as often as you like, as in
Blender) or joins it with its neighbour; every border between areas can be dragged, and a narrow 3D
view's header leaves out its less used controls. Right-click on a seam or an outline point for their
own menus; File ▸ Import Avatar OBJ… dresses a body mesh of your own (scaled to the avatar's height). N over the 3D view
opens its sidebar (the selected piece's placement as numbers); over the pattern editor it is the Sew
tool, as each Blender editor has its own keys. With the Select tool and pieces selected, G, R and S with
the pointer over them move, turn and scale them modally (X or Y locks the axis; typing a number sets
the move in mm, the turn in degrees or the scale factor exactly; click or Enter keeps, Esc or
right-click cancels and leaves nothing to redo); elsewhere these keys pick their tools. The 3D
header's mode selector switches between Garment Mode and Arrange Mode (arrangement points shown).
Drags and modal transforms are one undo step each; separate clicks are always separate steps.
After a dart, a rectangle, a fillet, a spread or a pleat, an Adjust Last Operation panel changes
its size. During G/R/S the status bar shows how far the selection has moved, turned or scaled. Pattern ▸ View ▸ Tool Names in Shelf
shows the tool shelves with labels. The pies are also under View ▸ Pie Menus. In the pattern editor: `W` select, `E` edit points,
`P` add point, `H` polygon, `S` rectangle, `D` dart, `U` cut a piece along a dragged line, `F` round
a corner (radius in the tool settings bar), `N` sew (`Shift N` free sew), `M` measure; hover a tool
for its name and key. Slash & Spread and Pleat (in the shelf and F3) take a line dragged across a
piece: Spread turns the side to the line's left about its end, opening a wedge at its start; Pleat
opens a knife pleat along it (opening and depth in the tool settings bar). Edit ▸ Preferences… (`Ctrl ,`) sets tooltips and tool names and rebinds any
command's shortcut (saved next to the fabric library). The Dart tool cuts a 20 × 80 mm dart into the clicked segment and sews its
legs; reshape it with Edit Points.

Over the 3D view, G and R move or turn the piece under the pointer (or the selected one) without
holding a button: X, Y or Z locks the axis, a typed number sets mm or degrees, click or Enter keeps,
Esc or right-click puts it back. A fabric can carry an image texture (Fabric tab ▸ Texture), tiled
along each piece's grain in the 3D view and in renders, and a normal map and a roughness map
(Fabric tab ▸ Surface: tile width, normal strength) that bend its shading and set its gloss texel by
texel, in solid shading and in the path tracer alike. File ▸ Import U3M Fabric… adds a fabric from a
U3M material (schema 1.0, a `.u3m` file beside its images or a `.u3ma` archive, unpacked into azam's
data folder): its colour, roughness, sheen and opacity, and its colour, normal and roughness images
with their tile widths (from the file, or from each image's resolution). U3M physics measurements
are not read; the fabric starts from the default preset's physics. The Grading panel lists the selected
point's offset in every size as an editable table.

The 3D header's third shading button (or View ▸ Rendered Shading) path-traces the view in place,
refining while nothing moves, as Blender's Rendered shading does. Garment ▸ Flatten Draped Piece lays
the selected piece's drape flat as a new piece, each mesh edge at its length on the body, and reports
how much larger or smaller it is than the draft. Simulation ▸ Record Animation records the frames
while the simulation plays; File ▸ Export Animation (.glb) writes them (with a moving avatar) as an
animated glTF that Blender and web viewers play back (sampled at 30 fps, frame offsets stored as
16-bit values with `KHR_mesh_quantization`). File ▸ Export FBX… writes the draped garment and the
avatar as binary FBX 7.4 (centimetres, +Y up), as Blender, Maya and game engines import it. Alembic
is not written; animated glTF carries the motion.

A piece's starting place around the avatar (slot, distance, angle, height) is in the Piece tab's
Arrangement panel; imported DXF pieces are placed by their names ("Back", "Sleeve L", "Collar"…).
Lengths show in the project's units (Project tab ▸ Units: millimetres, centimetres or inches): the
properties fields, segment labels, seam differences, the Measure tool and the tech pack. Area
layouts are saved per workspace with the preferences and come back next time (Edit ▸ Reset Layout
restores one). Workspaces are your own, as in Blender: the "+" after the tabs adds one of any kind,
and right-clicking a tab (or Edit ▸ Workspace) duplicates it with its layout, renames it
(double-click works too), moves it along the bar or deletes it; the tabs are saved with the
preferences. The Colorways editor (any area's editor menu, and beside the 3D view in the
Colorway workspace) shows a rendered card per colorway side by side; click one to make it active.
File ▸ Export DXF (All Sizes)… writes the graded nest: every piece in every size,
the sizes laid over each other. File ▸ Export ASTM DXF + Rules… writes the base size as an ASTM
D6673 DXF (turn points on layer 2, curve points on layer 3, each graded point tagged with its rule
number) and the matching `.rul` grade-rule table next to it, which pattern CAD systems read back as
a graded pattern. The Marker editor (in the Tech Pack workspace, or any area's editor menu) nests
every piece, as many copies as each is cut, on the fabric with the grain along the selvedge: set the
fabric width, the gap, whether pieces may turn 180° (not for one-way fabrics) and whether one
garment of every size goes in, and it shows the length of fabric the lay takes and its efficiency,
following every edit. A pair's second copy is laid flipped. File ▸ Export Marker DXF… / SVG… write
it for the cutting room. File ▸ Import DXF reads such a file back: when its boundary points carry rule
numbers and a `.rul` table of the same name sits next to it, the pieces come in graded (the
project takes the table's size run and sample size; METRIC and ENGLISH tables, DELTA rules), with
the cut line as the outline and no added allowance.

Sizes are graded in the Project tab (size run, base size) and the Piece tab's Grading panel (a step
per size for the selected points); the templates come graded from the size chart (the dress front with a side bust dart each side, its
side seams drafted to still match the back; the skirt and dress hems trued square to the side seams,
so the sewn hem runs straight through them), and the avatar
takes the body of the size shown (Avatar tab ▸ Follows Size). The Piece tab's Point panel places the
selected points by number and sets a straight segment's length. Length links keep two segments the same
length, or one a set amount longer (Segment panel ▸ Match Length Of, closest lengths first): front
and back side seams, a sleeve cap and its armhole. Edit either side and the other follows, its curve
scaled about its start corner so only its far corner moves; the pattern editor marks linked pairs
with the same "⇔ n" chip. Links hold in the base size. Piece ▸ Symmetric Editing (also on a piece's right-click
menu) edits a piece about its vertical centre line, drawn dashed: moving a point moves its mirror
partner too, and a point on the line stays on it. The 3D header's wireframe button (or
View ▸ Wireframe Overlay) draws the cloth and body meshes' edges. View ▸ Fit Map (or the 3D header in fit map shading) switches
between the Fit, Strain, Stress and Pressure maps. Graphics placed on pieces are printed on the cloth
in the 3D view and in renders (F12), together with topstitches, buttons, buttonholes and zippers
(the Zipper tool sews two edges and gives each a zipper half; in the simulation the cloth under its tape barely stretches or bends, and the zipper's weight hangs on the edge; long-range chords along the tape keep it from stretching at any mesh density). Pins set with the 3D Pin tool are saved
with the project. In the 3D view, drag a piece to move it before draping, Ctrl-drag to turn it around
the body (saved with the project and undoable), or show the arrangement points (header button or View menu) and click one to send the
selected pieces there. The Dart tool's width and depth sit in the pattern editor's header while it
is active. Fabrics can be deleted, saved to a fabric library shared by all projects
(`~/Library/Application Support/azam/fabrics` on macOS, or `$AZAM_LIBRARY/fabrics`) and added from
it; File ▸ Export glTF (.glb)… writes the draped garment with the avatar; DXF import also brings
notches and grain lines, and File ▸ Import SVG… makes pieces of a drawing's
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
- Native controls that stick out of their scroll container (or are mostly outside it sideways) hide
  instead of painting over what lies beyond it (the kit cannot cut them partially).
- Text fields show Persian and Arabic values shaped and in visual order while not being edited.
- A window repaints after its own key and pointer handlers call `notify` (it is out of the window
  list while it dispatches), and a frame delivered while the app is busy is requested again.

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
