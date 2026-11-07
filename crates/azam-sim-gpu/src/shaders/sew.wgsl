// One colour of sewing constraints: rate-limited while a wide seam closes, hard once closed.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  if (gid.x >= D.cnt) { return; }
  let s = SW[D.off + gid.x]; let u = bitcast<f32>(s.w); let u0 = 1.0 - u;
  let xa = X[s.x]; let x0 = X[s.y]; let x1 = X[s.z];
  let den = xa.w + u0 * u0 * x0.w + u * u * x1.w; if (den == 0.0) { return; }
  let d = u0 * x0.xyz + u * x1.xyz - xa.xyz; let L = length(d); if (L < 1e-6) { return; }
  var C = L; if (L > 10.0) { C = min(L, max(10.0, F.maxSew + L * 0.02)); }
  let s2 = C / L / den;
  X[s.x] = vec4<f32>(xa.xyz + xa.w * s2 * d, xa.w);
  X[s.y] = vec4<f32>(x0.xyz - x0.w * u0 * s2 * d, x0.w);
  X[s.z] = vec4<f32>(x1.xyz - x1.w * u * s2 * d, x1.w);
}
