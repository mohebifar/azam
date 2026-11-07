# MakeHuman CC0 avatar pack

`makehuman-cc0.bin` is the morph-target body the avatar uses when it is present (it falls back to
the built-in parametric mannequin otherwise). It is read by `azam_avatar::MhModel::parse` /
`MhModel::load_bundled` and needs no network access at run time.

- Size: 2,049,096 bytes (uncompressed).
- Built by `tools/mhpack` (Rust, workspace member `azam-mhpack`) from the sources below.

## Sources and licence

| What | Path in the source repository |
| --- | --- |
| Base mesh (body, eye helpers, joint helper cubes) | `makehuman/data/3dobjs/base.obj` |
| Skeleton (bone hierarchy, joint helpers) | `makehuman/data/rigs/default.mhskel` |
| Skin weights | `makehuman/data/rigs/default_weights.mhw` |
| Gender / ethnic macro targets (young) | `makehuman/data/targets/macrodetails/{african,asian,caucasian}-{female,male}-young.target` |
| Muscle / weight macro targets (young) | `makehuman/data/targets/macrodetails/universal-{female,male}-young-*.target` |
| Measurement targets | `makehuman/data/targets/measure/measure-*-{incr,decr}.target` |
| Nipple smoothing targets | `makehuman/data/targets/breast/nipple-{point,size}-decr.target` |

- Repository: <https://github.com/makehumancommunity/makehuman>
- Commit: `a8bc2d54ff0ac92e78ff71431b1023eda42bf482` (2024-06-26), pinned in `tools/mhpack/fetch.sh`
  and recorded in the pack header (`source`).
- Licence: **CC0 1.0 Universal** (public domain dedication). The repository's `LICENSE.md`
  (section C) releases the bundled assets — "the base mesh and proxies, targets and modifiers,
  textures, clothes, poses and expressions" — under CC0 1.0, with the full text in
  `LICENSE.ASSETS.md`. Each file used carries its own CC0 header (the `.target` and `.obj` files:
  "This asset was explicitly released as CC0 in september 2020"; `default.mhskel` and
  `default_weights.mhw`: `"license": "CC0"`).
- Only these CC0 data files are used. No MakeHuman program code (AGPL) is used or copied; in
  particular the measuring vertex lists of MakeHuman's measurement plugin are **not** used — the
  pack's measuring loops are derived independently (see below).

## What the pack holds

Millimetres, y up, the body facing +z, its left side on +x (MakeHuman's own axes, decimetres × 100).

- **Vertices**: the 13,380 `body` vertices, then the 144 vertices of `helper-l-eye` /
  `helper-r-eye` (drawn as eyes), then the 8-vertex joint helper cubes of `joint-l/r-shoulder`,
  `-elbow`, `-hand`, `-upper-leg`, `-knee`, `-ankle`, `joint-head`, `joint-neck`, `joint-pelvis`
  and `joint-spine-3` (not drawn; their centroids are the joints, as in MakeHuman, and the targets
  move them with the body).
- **Triangles**: the body and eye quads split in two, wound outwards.
- **Targets** (60): `g_female` / `g_male` — MakeHuman's default ethnic mix, i.e. the mean of the
  african, asian and caucasian `*-young` targets; the 18 `universal-{female,male}-young-*`
  muscle/weight targets; `-incr` and `-decr` of the 19 fit targets (all MakeHuman measurement
  targets except `measure-neck-height`); `nipple-point-decr` and `nipple-size-decr`.
  Displacements are quantised to 0.01 mm (`int16`).
- **Skin weights** (0–255): `w_armL/R` = the share of the vertex's weight on `upperarm01.L/R` and
  its descendants, `w_legL/R` likewise for `upperleg01.L/R` (weights normalised per vertex, as
  MakeHuman does). Optional extras for posing: `w_lowerarmL/R` (`lowerarm01.*` subtree),
  `w_lowerlegL/R` (`lowerleg01.*`), `w_spine` (`spine03` subtree: chest, arms, neck, head). Joint
  helper cubes get 255 in every array whose subtree contains a bone they head.
- **Measures, landmarks, joints**: derived on the base mesh by `tools/mhpack/src/derive.rs`:
  - *Girth loops*: each loop sits where its CC0 measure target acts — the magnitude-weighted centre
    of the vertices the `-incr` target moves by at least half its maximum (left side). The base
    mesh is cut there by a plane (horizontal for neck, bust, underbust, waist and hips;
    perpendicular to the bone, from the joint helpers, for upper arm, wrist, thigh, knee, calf and
    ankle); every crossed edge contributes its endpoint nearer the plane, and the loop is ordered
    by angle. Torso cuts skip arm vertices (arm weight ≥ 0.35), limb cuts keep only that limb's
    vertices (weight > 0.5).
  - `shoulderL` / `shoulderR`: the outer top corner of the shoulder — the vertex furthest
    up-and-out (60° above horizontal) from the shoulder joint within 90 mm of it, in a frontal
    slab through the joint; mirrored for the right side.
  - `crotch`: the lowest vertex on the midline (x = 0).
  - Lengths (vertex pairs): upper arm = shoulder point → outer elbow; lower arm = outer elbow →
    outer wrist (outermost skin vertex, perpendicular to the bone, in a thin slab through the
    joint); shoulder = side of the neck loop → shoulder point; across front = outermost
    front-facing torso vertex left and right at the height of the across-front target; nape to
    waist = back midline of the neck loop → back midline of the waist loop; waist to hip = side of
    the waist loop → side of the hip loop; upper leg = side of the hip loop → outer knee; lower
    leg (knee to floor) = outer knee → lowest foot vertex.

  The exact landmark indices of other packs in this format are not derivable from public data,
  hence these definitions; they are consistent with how the avatar reads each measurement
  (convex-hull tape for girths, straight distances for lengths).

## Rebuilding

```sh
tools/mhpack/fetch.sh target/makehuman           # sparse, pinned checkout (git)
cargo run -p azam-mhpack --release -- --makehuman target/makehuman \
    --source "$(tools/mhpack/fetch.sh --source)" \
    --out assets/makehuman/makehuman-cc0.bin --check --verbose
```

The build is deterministic. `--check` parses the result and fits the female and male presets,
printing every measurement and the largest deviation; `--verbose` prints how each measure and
landmark was placed.

At run time the pack is looked for at `$AZAM_MAKEHUMAN_PACK`, then `assets/makehuman/` under the
working directory, under the executable's directory and its parents, and in the source tree.
