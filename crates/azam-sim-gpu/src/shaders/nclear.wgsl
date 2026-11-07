// Pressure normals: clears the fixed-point normal accumulators.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) { let i = gid.x; if (i >= F.N * 4u) { return; } NRM[i] = 0; }
