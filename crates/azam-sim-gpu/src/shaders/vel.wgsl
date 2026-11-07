// Velocities from the substep's displacement.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let q = gid.x; if (q >= F.N) { return; }
  V[q] = vec4<f32>((X[q].xyz - P[q].xyz) / F.h, 0.0);
}
