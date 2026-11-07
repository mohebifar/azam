// Triangle hash: clears the bucket counts and the contact counter.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) { let i = gid.x; if (i < F.HS) { HC[i] = 0u; } if (i == 0u) { CTRL[0] = 0u; } }
