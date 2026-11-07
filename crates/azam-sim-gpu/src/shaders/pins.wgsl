// Puts every pinned particle back at its pin.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let e = gid.x; if (e >= F.nPin) { return; }
  let pn = PIN[e]; let q = bitcast<u32>(pn.w); X[q] = vec4<f32>(pn.xyz, X[q].w);
}
