// Substep: keeps free particles out of the body (with friction along the skin) and above the floor.
// With a moving body (F.bodyMot), friction acts on the motion relative to the skin and a pushed
// particle leaves no faster along the normal than the skin moves.

// Trilinear sample of the body surface velocity grid (zero outside it).
fn bvel(p: vec3<f32>) -> vec3<f32> {
  let h0 = BVEL[0]; let h1 = BVEL[1];
  let nx = u32(h1.x); let ny = u32(h1.y); let nz = u32(h1.z);
  let f = (p - h0.xyz) / h0.w;
  if (f.x < 0.0 || f.y < 0.0 || f.z < 0.0 || f.x >= f32(nx - 1u) || f.y >= f32(ny - 1u) || f.z >= f32(nz - 1u)) { return vec3<f32>(0.0); }
  let i = u32(f.x); let j = u32(f.y); let k = u32(f.z);
  let u = f.x - f32(i); let v = f.y - f32(j); let w = f.z - f32(k);
  let nxy = nx * ny; let o = 2u + i + nx * j + nxy * k;
  let x00 = mix(BVEL[o].xyz, BVEL[o + 1u].xyz, u); let x10 = mix(BVEL[o + nx].xyz, BVEL[o + nx + 1u].xyz, u);
  let x01 = mix(BVEL[o + nxy].xyz, BVEL[o + nxy + 1u].xyz, u); let x11 = mix(BVEL[o + nxy + nx].xyz, BVEL[o + nxy + nx + 1u].xyz, u);
  return mix(mix(x00, x10, v), mix(x01, x11, v), w);
}

@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let q = gid.x; if (q >= F.N) { return; }
  let x = X[q]; if (x.w == 0.0) { return; }
  let m = PP[q].y; let pr = P[q].xyz; var pos = x.xyz;
  let s = gsamp(pos);
  if (s.w < m) {
    let L0 = length(s.xyz); let n = s.xyz / select(L0, 1.0, L0 == 0.0);
    pos = pos + n * (m - s.w);
    var b = vec3<f32>(0.0);
    if (F.bodyMot != 0u) { b = bvel(pos) * F.h; }
    let dd = pos - pr - b; let t = dd - dot(dd, n) * n;
    pos = pos - t * PP[q].z;
    if (F.bodyMot != 0u) {
      let along = dot(pos - pr, n); let allowed = max(dot(b, n), 0.0);
      if (along > allowed) { P[q] = vec4<f32>(pr + n * (along - allowed), P[q].w); }
    }
  }
  if (pos.y < 1.0 + m * 0.3) { pos = vec3<f32>(pr.x + (pos.x - pr.x) * 0.5, 1.0 + m * 0.3, pr.z + (pos.z - pr.z) * 0.5); }
  X[q] = vec4<f32>(pos, x.w);
}
