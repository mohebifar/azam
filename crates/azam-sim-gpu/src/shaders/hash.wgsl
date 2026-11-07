// Spatial hash buckets of integer cells: the cloth-triangle table and the skin-vertex table.
fn hk(i: i32, j: i32, k: i32) -> u32 { return ((bitcast<u32>(i) * 73856093u) ^ (bitcast<u32>(j) * 19349663u) ^ (bitcast<u32>(k) * 83492791u)) & (F.HS - 1u); }
fn hks(i: i32, j: i32, k: i32) -> u32 { return ((bitcast<u32>(i) * 73856093u) ^ (bitcast<u32>(j) * 19349663u) ^ (bitcast<u32>(k) * 83492791u)) & (F.skinHS - 1u); }
