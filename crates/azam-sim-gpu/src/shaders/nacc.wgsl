// Pressure normals: adds every triangle's area normal to its corners (fixed point, 1/64).
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let t = gid.x; if (t >= F.nTri) { return; }
  let tr = TRI[t]; let a = X[tr.x].xyz; let n = cross(X[tr.y].xyz - a, X[tr.z].xyz - a) * 64.0;
  let ni = vec3<i32>(round(n));
  for (var e = 0u; e < 3u; e = e + 1u) { let v = select(select(tr.z, tr.y, e == 1u), tr.x, e == 0u);
    atomicAdd(&NRM[4u * v], ni.x); atomicAdd(&NRM[4u * v + 1u], ni.y); atomicAdd(&NRM[4u * v + 2u], ni.z); }
}
