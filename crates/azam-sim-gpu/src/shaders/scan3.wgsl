// Triangle hash: adds the block offsets; bucket starts go to HASH (and HC, as scatter cursors).
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x; if (i >= F.HS) { return; }
  let s = HASH[i] + BS[i / 256u]; HASH[i] = s; HC[i] = s;
  if (i == F.HS - 1u) { HASH[F.HS] = BS[256]; }
}
