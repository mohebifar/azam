// Kinetic energy partial sums per workgroup: squared speed and count of the free particles.
var<workgroup> se: array<vec2<f32>, 128>;
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {
  let q = gid.x; var s = vec2<f32>(0.0);
  if (q < F.N) { if (X[q].w > 0.0) { let v = V[q].xyz; s = vec2<f32>(dot(v, v), 1.0); } }
  se[lid.x] = s; workgroupBarrier();
  for (var off = 64u; off > 0u; off = off / 2u) { if (lid.x < off) { se[lid.x] = se[lid.x] + se[lid.x + off]; } workgroupBarrier(); }
  if (lid.x == 0u) { PART[wid.x] = se[0]; }
}
