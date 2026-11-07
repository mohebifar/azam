// End-of-substep guard: corrects only true penetration (inside 35 % of the margin), without
// friction, and keeps particles above the floor.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let q = gid.x; if (q >= F.N) { return; }
  let x = X[q]; if (x.w == 0.0) { return; }
  var pos = x.xyz; let s = gsamp(pos); let m2 = PP[q].y * 0.35;
  if (s.w < m2) { let L0 = length(s.xyz); pos = pos + s.xyz / select(L0, 1.0, L0 == 0.0) * (m2 - s.w); }
  if (pos.y < 1.0) { pos.y = 1.0; }
  X[q] = vec4<f32>(pos, x.w);
}
