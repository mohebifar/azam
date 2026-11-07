// Triangle hash: counts the entries of every bucket (each triangle in every cell its bounds,
// grown by the search radius, cover).
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let t = gid.x; if (t >= F.nTriG) { return; }
  let tr = TRI[t]; let a = X[tr.x].xyz; let b = X[tr.y].xyz; let c = X[tr.z].xyz;
  let i0 = vec3<i32>(floor((min(min(a, b), c) - vec3<f32>(F.R)) / F.cellC)); let i1 = vec3<i32>(floor((max(max(a, b), c) + vec3<f32>(F.R)) / F.cellC));
  let nc = (i1 - i0) + vec3<i32>(1); if (nc.x * nc.y * nc.z > 64) { return; }
  for (var i = i0.x; i <= i1.x; i = i + 1) { for (var j = i0.y; j <= i1.y; j = j + 1) { for (var k = i0.z; k <= i1.z; k = k + 1) { atomicAdd(&HC[hk(i, j, k)], 1u); } } }
}
