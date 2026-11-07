// Pulls every grabbed particle towards the grab point plus its offset.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let e = gid.x; if (e >= F.nGrab) { return; }
  let g = GR[2u * e]; let q = bitcast<u32>(GR[2u * e + 1u].x);
  let x = X[q]; let tgt = vec3<f32>(F.tx, F.ty, F.tz) + g.xyz;
  X[q] = vec4<f32>(x.xyz + (tgt - x.xyz) * g.w, x.w);
}
