// Per frame: seam gap reduction over every second sewing constraint, globally and per piece
// (fixed point, 1/16 mm).
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let c = gid.x * 2u; if (c >= F.nSew) { return; }
  let s = SW[c]; let u = bitcast<f32>(s.w);
  let L = length((1.0 - u) * X[s.y].xyz + u * X[s.z].xyz - X[s.x].xyz);
  let fx = u32(min(L, 2000.0) * 16.0);
  atomicAdd(&CTRL[1], fx); atomicAdd(&CTRL[2], 1u);
  let pa = min(PD[2u * s.x].x, 63u); let pb = min(PD[2u * s.y].x, 63u);
  atomicAdd(&CTRL[4u + pa], fx); atomicAdd(&CTRL[68u + pa], 1u);
  if (pb != pa) { atomicAdd(&CTRL[4u + pb], fx); atomicAdd(&CTRL[68u + pb], 1u); }
}
