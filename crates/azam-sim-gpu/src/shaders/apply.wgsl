// Applies the accumulated corrections, averaged over their count; with flag 1 the substep start
// moves too, so the correction adds no velocity.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let q = gid.x; if (q >= F.N) { return; }
  let c = atomicExchange(&DELTA[4u * q + 3u], 0);
  if (c == 0) { return; }
  let sx = atomicExchange(&DELTA[4u * q], 0); let sy = atomicExchange(&DELTA[4u * q + 1u], 0); let sz = atomicExchange(&DELTA[4u * q + 2u], 0);
  let d = vec3<f32>(f32(sx), f32(sy), f32(sz)) / (SCALE * f32(c));
  let x = X[q]; X[q] = vec4<f32>(x.xyz + d, x.w);
  if (D.flag == 1u) { let p = P[q]; P[q] = vec4<f32>(p.xyz + d, p.w); }
}
