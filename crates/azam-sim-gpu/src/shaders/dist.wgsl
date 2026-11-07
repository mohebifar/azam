// One colour of distance constraints: stretch (XPBD, kind 0), bending (resists folding only, or
// two-way on fold lines, kind 1) and fasteners (pulled to a 2 mm gap at the sewing speed, kind 2).
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  if (gid.x >= D.cnt) { return; }
  let c = CON[D.off + gid.x];
  let xi = X[c.i]; let xj = X[c.j]; let wi = xi.w; let wj = xj.w; let ws = wi + wj;
  if (ws == 0.0) { return; }
  let d = xj.xyz - xi.xyz; let L = length(d);
  var dl = 0.0;
  if (c.kind == 0u) {
    if (L < 1e-9) { return; }
    let C = L - c.L; let al = c.r * select(1.0, c.cm, C < 0.0) * F.ih2;
    dl = -C / (ws + al) / L;
  } else if (c.kind == 1u) {
    if (L < 1e-9) { return; }
    let C = L - c.L; if (c.r >= 0.0 && C > 0.0) { return; }
    dl = -C / (ws + abs(c.r) * ws) / L;
  } else {
    if (L < 2.0) { return; }
    dl = -min(L - 2.0, F.maxSew) / L / ws;
  }
  X[c.i] = vec4<f32>(xi.xyz - wi * dl * d, wi);
  X[c.j] = vec4<f32>(xj.xyz + wj * dl * d, wj);
}
