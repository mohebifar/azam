//! Binary FBX (7.4) export of the draped garment and the avatar: one mesh per piece with its
//! fabric's colour as a material, normals and texture coordinates, in centimetres with +Y up
//! (FBX's defaults), for Blender, Maya, Marvelous-style pipelines and game engines.

use crate::format::hex_rgb;
use crate::obj::{ObjMesh, ObjPiece};

/// FBX file version written (7.4, read by Blender, Maya and Unity alike).
const VERSION: u32 = 7400;
/// [`VERSION`] as the signed integer the header node stores.
const VERSION_I32: i32 = 7400;
/// FBX positions are centimetres; meshes are in mm.
const CM_PER_MM: f64 = 0.1;
/// Skin colour of an exported avatar.
const SKIN: [f64; 3] = [0.71, 0.69, 0.67];

/// A property of an FBX node.
enum Prop {
    Int(i32),
    Long(i64),
    Double(f64),
    Str(Vec<u8>),
    Doubles(Vec<f64>),
    Ints(Vec<i32>),
}

/// An FBX node: a name, properties and child nodes.
struct Node {
    name: &'static str,
    props: Vec<Prop>,
    children: Vec<Node>,
}

fn node(name: &'static str, props: Vec<Prop>, children: Vec<Node>) -> Node {
    Node {
        name,
        props,
        children,
    }
}

fn s(text: &str) -> Prop {
    Prop::Str(text.as_bytes().to_vec())
}

/// An object name as FBX stores it: `name`, a NUL, a 0x01 and the class.
fn object_name(name: &str, class: &str) -> Prop {
    let mut b = name.as_bytes().to_vec();
    b.extend_from_slice(&[0, 1]);
    b.extend_from_slice(class.as_bytes());
    Prop::Str(b)
}

/// A `P` entry of a `Properties70` block.
fn p(name: &str, kind: &str, label: &str, flags: &str, values: Vec<Prop>) -> Node {
    let mut props = vec![s(name), s(kind), s(label), s(flags)];
    props.extend(values);
    node("P", props, Vec::new())
}

fn write_prop(out: &mut Vec<u8>, prop: &Prop) {
    let array = |out: &mut Vec<u8>, tag: u8, count: usize, bytes: Vec<u8>| {
        out.push(tag);
        out.extend_from_slice(&(count as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // not compressed
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
    };
    match prop {
        Prop::Int(v) => {
            out.push(b'I');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::Long(v) => {
            out.push(b'L');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::Double(v) => {
            out.push(b'D');
            out.extend_from_slice(&v.to_le_bytes());
        }
        Prop::Str(b) => {
            out.push(b'S');
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        Prop::Doubles(v) => array(
            out,
            b'd',
            v.len(),
            v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        ),
        Prop::Ints(v) => array(
            out,
            b'i',
            v.len(),
            v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        ),
    }
}

/// Writes `n` (offsets are absolute in the file, so `out` must hold the file from its start).
fn write_node(out: &mut Vec<u8>, n: &Node) {
    let start = out.len();
    out.extend_from_slice(&[0; 12]); // end offset, property count, property bytes: patched below
    out.push(n.name.len() as u8);
    out.extend_from_slice(n.name.as_bytes());
    let props_at = out.len();
    for prop in &n.props {
        write_prop(out, prop);
    }
    let props_len = out.len() - props_at;
    if !n.children.is_empty() {
        for c in &n.children {
            write_node(out, c);
        }
        out.extend_from_slice(&[0; 13]); // the null record ending the children
    }
    let end = out.len() as u32;
    out[start..start + 4].copy_from_slice(&end.to_le_bytes());
    out[start + 4..start + 8].copy_from_slice(&(n.props.len() as u32).to_le_bytes());
    out[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
}

/// One mesh to write.
struct Part<'a> {
    name: String,
    positions: &'a [[f32; 3]],
    normals: &'a [[f32; 3]],
    uvs: Option<Vec<f64>>,
    triangles: &'a [[u32; 3]],
    material: (String, [f64; 3], f64),
}

fn color(hex: &str) -> [f64; 3] {
    let c = hex_rgb(hex).unwrap_or([204, 204, 204]);
    c.map(|v| f64::from(v) / 255.0)
}

/// A vertex index as FBX stores it (a 32-bit signed integer).
fn index(i: u32) -> i32 {
    i32::try_from(i).unwrap_or(i32::MAX)
}

/// A layer element of a geometry (normals, UVs, materials): its mapping and the data node.
fn layer_element(kind: &'static str, name: &str, (mapping, reference): (&str, &str), data: Node) -> Node {
    node(
        kind,
        vec![Prop::Int(0)],
        vec![
            node("Version", vec![Prop::Int(101)], vec![]),
            node("Name", vec![s(name)], vec![]),
            node("MappingInformationType", vec![s(mapping)], vec![]),
            node("ReferenceInformationType", vec![s(reference)], vec![]),
            data,
        ],
    )
}

/// The geometry, model and material nodes of `part`, with ids `geo`, `model` and `mat`.
fn part_nodes(part: &Part, geo: i64, model: i64, mat: i64) -> Vec<Node> {
    let mut out = Vec::new();
    let vertices: Vec<f64> = part
        .positions
        .iter()
        .flatten()
        .map(|v| f64::from(*v) * CM_PER_MM)
        .collect();
    let index: Vec<i32> = part
        .triangles
        .iter()
        .flat_map(|t| [index(t[0]), index(t[1]), !index(t[2])])
        .collect();
    let normals: Vec<f64> = part.normals.iter().flatten().map(|v| f64::from(*v)).collect();
    let mut layers = vec![layer_element(
        "LayerElementNormal",
        "",
        ("ByVertice", "Direct"),
        node("Normals", vec![Prop::Doubles(normals)], vec![]),
    )];
    if let Some(uv) = &part.uvs {
        layers.push(layer_element(
            "LayerElementUV",
            "UVMap",
            ("ByVertice", "Direct"),
            node("UV", vec![Prop::Doubles(uv.clone())], vec![]),
        ));
    }
    layers.push(layer_element(
        "LayerElementMaterial",
        "",
        ("AllSame", "IndexToDirect"),
        node("Materials", vec![Prop::Ints(vec![0])], vec![]),
    ));
    let layer_refs: Vec<Node> = layers
        .iter()
        .map(|l| {
            node(
                "LayerElement",
                vec![],
                vec![
                    node("Type", vec![s(l.name)], vec![]),
                    node("TypedIndex", vec![Prop::Int(0)], vec![]),
                ],
            )
        })
        .collect();
    let mut geo_children = vec![
        node("Vertices", vec![Prop::Doubles(vertices)], vec![]),
        node("PolygonVertexIndex", vec![Prop::Ints(index)], vec![]),
        node("GeometryVersion", vec![Prop::Int(124)], vec![]),
    ];
    geo_children.extend(layers);
    let mut layer = vec![node("Version", vec![Prop::Int(100)], vec![])];
    layer.extend(layer_refs);
    geo_children.push(node("Layer", vec![Prop::Int(0)], layer));
    out.push(node(
        "Geometry",
        vec![Prop::Long(geo), object_name(&part.name, "Geometry"), s("Mesh")],
        geo_children,
    ));
    out.push(node(
        "Model",
        vec![Prop::Long(model), object_name(&part.name, "Model"), s("Mesh")],
        vec![
            node("Version", vec![Prop::Int(232)], vec![]),
            node("Properties70", vec![], vec![]),
            node("Shading", vec![Prop::Int(1)], vec![]),
            node("Culling", vec![s("CullingOff")], vec![]),
        ],
    ));
    let (mat_name, rgb, alpha) = &part.material;
    let rgbp = |c: [f64; 3]| c.iter().map(|v| Prop::Double(*v)).collect::<Vec<_>>();
    out.push(node(
        "Material",
        vec![Prop::Long(mat), object_name(mat_name, "Material"), s("")],
        vec![
            node("Version", vec![Prop::Int(102)], vec![]),
            node("ShadingModel", vec![s("lambert")], vec![]),
            node(
                "Properties70",
                vec![],
                vec![
                    p("DiffuseColor", "Color", "", "A", rgbp(*rgb)),
                    p("Opacity", "double", "Number", "", vec![Prop::Double(*alpha)]),
                ],
            ),
        ],
    ));
    out
}

/// The `FBXHeaderExtension` and `GlobalSettings` nodes: version, creator, +Y up, centimetres.
fn header_nodes() -> Vec<Node> {
    let int_p = |name: &str, v: i32| p(name, "int", "Integer", "", vec![Prop::Int(v)]);
    vec![
        node(
            "FBXHeaderExtension",
            vec![],
            vec![
                node("FBXHeaderVersion", vec![Prop::Int(1003)], vec![]),
                node("FBXVersion", vec![Prop::Int(VERSION_I32)], vec![]),
                node("Creator", vec![s("azam")], vec![]),
            ],
        ),
        node(
            "GlobalSettings",
            vec![],
            vec![
                node("Version", vec![Prop::Int(1000)], vec![]),
                node(
                    "Properties70",
                    vec![],
                    vec![
                        int_p("UpAxis", 1),
                        int_p("UpAxisSign", 1),
                        int_p("FrontAxis", 2),
                        int_p("FrontAxisSign", 1),
                        int_p("CoordAxis", 0),
                        int_p("CoordAxisSign", 1),
                        p("UnitScaleFactor", "double", "Number", "", vec![Prop::Double(1.0)]),
                    ],
                ),
            ],
        ),
    ]
}

/// The file's closing bytes: an end record, the footer id, padding, the version and a magic.
fn write_footer(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0; 13]);
    out.extend_from_slice(&[
        0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e,
    ]);
    out.extend_from_slice(&[0; 4]);
    while out.len() % 16 != 0 {
        out.push(0);
    }
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&[0; 120]);
    out.extend_from_slice(&[
        0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b,
    ]);
}

/// The draped pieces (and optionally the avatar) as a binary FBX file.
#[must_use]
pub fn export_fbx(pieces: &[ObjPiece], avatar: Option<&ObjMesh>) -> Vec<u8> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for uv in pieces.iter().flat_map(|p| p.uvs) {
        (x0, y0, x1, y1) = (x0.min(uv[0]), y0.min(uv[1]), x1.max(uv[0]), y1.max(uv[1]));
    }
    let span = (x1 - x0).max(y1 - y0);
    let span = if span > 0.0 { span } else { 1.0 };
    let mut parts: Vec<Part> = pieces
        .iter()
        .filter(|p| !p.hidden)
        .map(|p| Part {
            name: p.name.to_owned(),
            positions: p.positions,
            normals: p.normals,
            uvs: Some(
                p.uvs
                    .iter()
                    .flat_map(|t| [(t[0] - x0) / span, 1.0 - (t[1] - y0) / span])
                    .collect(),
            ),
            triangles: p.triangles,
            material: (
                p.material.name.clone(),
                color(&p.material.color),
                p.material.opacity,
            ),
        })
        .collect();
    if let Some(av) = avatar {
        parts.push(Part {
            name: "Avatar".into(),
            positions: av.positions,
            normals: av.normals,
            uvs: None,
            triangles: av.triangles,
            material: ("avatar_skin".into(), SKIN, 1.0),
        });
    }
    let (mut objects, mut connections) = (Vec::new(), Vec::new());
    let mut next_id: i64 = 1_000_000;
    let mut fresh = || {
        next_id += 1;
        next_id
    };
    for part in &parts {
        let (geo, model, mat) = (fresh(), fresh(), fresh());
        objects.extend(part_nodes(part, geo, model, mat));
        for (child, parent) in [(geo, model), (model, 0), (mat, model)] {
            connections.push(node(
                "C",
                vec![s("OO"), Prop::Long(child), Prop::Long(parent)],
                vec![],
            ));
        }
    }
    let mut top = header_nodes();
    top.extend([
        node("Objects", vec![], objects),
        node("Connections", vec![], connections),
    ]);
    let mut out = b"Kaydara FBX Binary  \x00\x1a\x00".to_vec();
    out.extend_from_slice(&VERSION.to_le_bytes());
    for n in &top {
        write_node(&mut out, n);
    }
    write_footer(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::ObjMaterial;

    /// Reads the top-level node names and their end offsets back.
    fn top_nodes(b: &[u8]) -> Vec<(String, usize)> {
        let mut at = 27;
        let mut out = Vec::new();
        loop {
            let end = u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes")) as usize;
            if end == 0 {
                break;
            }
            let len = b[at + 12] as usize;
            out.push((
                String::from_utf8_lossy(&b[at + 13..at + 13 + len]).into_owned(),
                end,
            ));
            at = end;
        }
        out
    }

    #[test]
    fn a_piece_becomes_a_binary_fbx() {
        let positions = [[0.0, 1000.0, 0.0], [100.0, 1000.0, 0.0], [0.0, 900.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let piece = ObjPiece {
            name: "Front",
            material: ObjMaterial {
                name: "jersey".into(),
                color: "#ff0000".into(),
                opacity: 1.0,
            },
            hidden: false,
            positions: &positions,
            normals: &normals,
            uvs: &[[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]],
            triangles: &[[0, 1, 2]],
        };
        let fbx = export_fbx(&[piece], None);
        assert!(fbx.starts_with(b"Kaydara FBX Binary  \x00"));
        assert_eq!(
            u32::from_le_bytes(fbx[23..27].try_into().expect("4 bytes")),
            VERSION
        );
        let names: Vec<String> = top_nodes(&fbx).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            ["FBXHeaderExtension", "GlobalSettings", "Objects", "Connections"]
        );
        assert!(fbx.ends_with(&[0x75, 0x8f, 0x29, 0x0b]));
    }
}
