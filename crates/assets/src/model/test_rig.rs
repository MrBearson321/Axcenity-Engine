//! A small rigged and animated FBX file, for tests and for the
//! `hello-animation` example (`examples/hello-animation/assets/models/noodle.fbx`
//! is this file, checked by a test).
//!
//! "Noodle" is a 1×3×1 column (Y up, in meters) skinned to a chain of three
//! bones, Root (at the bottom), Mid (1 up) and Top (2 up), with a rigid
//! "Hat" cube on Top that isn't skinned. It has three takes: Idle (2 s,
//! Mid sways 15° either way around Z), Moves (2 s: Top waves 60° in the
//! first second, then the whole column hops 1 m) and Walk (1 s, Root moves
//! 2 m forward along +Z).

use std::fmt::Write;

/// FBX time units per second.
const KTIME: i64 = 46_186_158_000;

/// One animated channel: which model's property and axis, and its keys
/// (seconds, value).
struct Curve {
    model: u32,
    property: &'static str,
    axis: &'static str,
    keys: &'static [(f64, f64)],
}

/// A take's name, its length in seconds and its curves.
struct TakeSpec {
    name: &'static str,
    seconds: f64,
    curves: &'static [Curve],
}

const ROOT: u32 = 2001;
const MID: u32 = 2002;
const TOP: u32 = 2003;

const TAKES: &[TakeSpec] = &[
    TakeSpec {
        name: "Idle",
        seconds: 2.0,
        curves: &[Curve {
            model: MID,
            property: "Lcl Rotation",
            axis: "Z",
            keys: &[
                (0.0, 0.0),
                (0.5, 15.0),
                (1.0, 0.0),
                (1.5, -15.0),
                (2.0, 0.0),
            ],
        }],
    },
    TakeSpec {
        name: "Moves",
        seconds: 2.0,
        curves: &[
            Curve {
                model: TOP,
                property: "Lcl Rotation",
                axis: "Z",
                keys: &[(0.0, 0.0), (0.5, 60.0), (1.0, 0.0)],
            },
            Curve {
                model: ROOT,
                property: "Lcl Translation",
                axis: "Y",
                keys: &[(0.0, 0.0), (1.0, 0.0), (1.5, 1.0), (2.0, 0.0)],
            },
        ],
    },
    TakeSpec {
        name: "Walk",
        seconds: 1.0,
        curves: &[Curve {
            model: ROOT,
            property: "Lcl Translation",
            axis: "Z",
            keys: &[(0.0, 0.0), (1.0, 2.0)],
        }],
    },
];

fn list<T: ToString>(values: impl IntoIterator<Item = T>) -> String {
    values
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// A 4×4 matrix moving by `y` up, as FBX writes them (column by column).
fn up(y: f64) -> String {
    list([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, y, 0.0, 1.0,
    ])
}

/// The noodle's ASCII FBX file (see the module docs).
pub(crate) fn noodle_fbx() -> String {
    // The column: 4 rings of 4 corners, at y = 0, 1, 2 and 3.
    let corners = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    let mut vertices = Vec::new();
    for ring in 0..4 {
        for (x, z) in corners {
            vertices.extend([x, f64::from(ring), z]);
        }
    }
    // Each face's corners, counter-clockwise seen from outside; FBX marks a
    // face's last corner by writing it as -(index + 1).
    let mut faces: Vec<[i32; 4]> = Vec::new();
    for ring in 0..3 {
        for corner in 0..4 {
            let next = (corner + 1) % 4;
            faces.push([
                ring * 4 + corner,
                (ring + 1) * 4 + corner,
                (ring + 1) * 4 + next,
                ring * 4 + next,
            ]);
        }
    }
    faces.push([0, 1, 2, 3]);
    faces.push([12, 15, 14, 13]);
    let polygons = list(faces.iter().flat_map(|face| {
        let [a, b, c, d] = *face;
        [a, b, c, -(d + 1)]
    }));
    let hat = list(
        [
            -1, -1, -1, 1, -1, -1, 1, 1, -1, -1, 1, -1, -1, -1, 1, 1, -1, 1, 1, 1, 1, -1, 1, 1,
        ]
        .map(|v| f64::from(v) * 0.4),
    );

    // Each bone's skin: the corners it moves and how much.
    let skin = [
        (ROOT, 0.0, vec![(0..4, 1.0), (4..8, 0.5)]),
        (MID, 1.0, vec![(4..8, 0.5), (8..12, 0.5)]),
        (TOP, 2.0, vec![(8..12, 0.5), (12..16, 1.0)]),
    ];
    let mut objects = String::new();
    let mut connections = String::new();
    for (i, (bone, height, weights)) in skin.iter().enumerate() {
        let id = 3001 + i as u32;
        let indices: Vec<i32> = weights.iter().flat_map(|(r, _)| r.clone()).collect();
        let values: Vec<f64> = weights
            .iter()
            .flat_map(|(r, w)| r.clone().map(move |_| *w))
            .collect();
        writeln!(
            objects,
            "\tDeformer: {id}, \"SubDeformer::Cluster{i}\", \"Cluster\" {{\n\
             \t\tVersion: 100\n\
             \t\tIndexes: *{n} {{\n\t\t\ta: {indices}\n\t\t}}\n\
             \t\tWeights: *{n} {{\n\t\t\ta: {values}\n\t\t}}\n\
             \t\tTransform: *16 {{\n\t\t\ta: {inverse}\n\t\t}}\n\
             \t\tTransformLink: *16 {{\n\t\t\ta: {link}\n\t\t}}\n\
             \t}}",
            n = indices.len(),
            indices = list(&indices),
            values = list(&values),
            // From the mesh into the bone, at bind time: the inverse of the
            // bone's place (the mesh is at the origin).
            inverse = up(-height),
            link = up(*height),
        )
        .unwrap();
        writeln!(connections, "\tC: \"OO\",{id},3000").unwrap();
        writeln!(connections, "\tC: \"OO\",{bone},{id}").unwrap();
    }

    // The takes: a stack and a layer each, a curve node per animated
    // property, and a curve per animated axis.
    for (t, take) in TAKES.iter().enumerate() {
        let stack = 6000 + t as u32 * 100;
        let layer = stack + 1;
        let stop = (take.seconds * KTIME as f64) as i64;
        writeln!(
            objects,
            "\tAnimationStack: {stack}, \"AnimStack::{name}\", \"\" {{\n\
             \t\tProperties70:  {{\n\
             \t\t\tP: \"LocalStart\", \"KTime\", \"Time\", \"\",0\n\
             \t\t\tP: \"LocalStop\", \"KTime\", \"Time\", \"\",{stop}\n\
             \t\t\tP: \"ReferenceStart\", \"KTime\", \"Time\", \"\",0\n\
             \t\t\tP: \"ReferenceStop\", \"KTime\", \"Time\", \"\",{stop}\n\
             \t\t}}\n\
             \t}}\n\
             \tAnimationLayer: {layer}, \"AnimLayer::BaseLayer\", \"\" {{\n\t}}",
            name = take.name,
        )
        .unwrap();
        writeln!(connections, "\tC: \"OO\",{layer},{stack}").unwrap();
        for (c, curve) in take.curves.iter().enumerate() {
            let node = stack + 10 + c as u32 * 2;
            let id = node + 1;
            let short = if curve.property == "Lcl Rotation" {
                "R"
            } else {
                "T"
            };
            let times = list(curve.keys.iter().map(|(s, _)| (s * KTIME as f64) as i64));
            let values = list(curve.keys.iter().map(|(_, v)| v));
            writeln!(
                objects,
                "\tAnimationCurveNode: {node}, \"AnimCurveNode::{short}\", \"\" {{\n\
                 \t\tProperties70:  {{\n\
                 \t\t\tP: \"d|X\", \"Number\", \"\", \"A\",0\n\
                 \t\t\tP: \"d|Y\", \"Number\", \"\", \"A\",0\n\
                 \t\t\tP: \"d|Z\", \"Number\", \"\", \"A\",0\n\
                 \t\t}}\n\
                 \t}}\n\
                 \tAnimationCurve: {id}, \"AnimCurve::\", \"\" {{\n\
                 \t\tDefault: 0\n\
                 \t\tKeyVer: 4009\n\
                 \t\tKeyTime: *{n} {{\n\t\t\ta: {times}\n\t\t}}\n\
                 \t\tKeyValueFloat: *{n} {{\n\t\t\ta: {values}\n\t\t}}\n\
                 \t\tKeyAttrFlags: *1 {{\n\t\t\ta: 4\n\t\t}}\n\
                 \t\tKeyAttrDataFloat: *4 {{\n\t\t\ta: 0,0,0,0\n\t\t}}\n\
                 \t\tKeyAttrRefCount: *1 {{\n\t\t\ta: {n}\n\t\t}}\n\
                 \t}}",
                n = curve.keys.len(),
            )
            .unwrap();
            writeln!(connections, "\tC: \"OO\",{node},{layer}").unwrap();
            writeln!(
                connections,
                "\tC: \"OP\",{node},{model}, \"{property}\"",
                model = curve.model,
                property = curve.property
            )
            .unwrap();
            writeln!(connections, "\tC: \"OP\",{id},{node}, \"d|{}\"", curve.axis).unwrap();
        }
    }

    format!(
        r#"; FBX 7.4.0 project file
; Noodle: a rigged test model made by axcenity-assets (model/test_rig.rs).
FBXHeaderExtension:  {{
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}}
GlobalSettings:  {{
	Version: 1000
	Properties70:  {{
		P: "UpAxis", "int", "Integer", "",1
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",2
		P: "FrontAxisSign", "int", "Integer", "",1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",100
		P: "TimeMode", "enum", "", "",6
	}}
}}
Objects:  {{
	Geometry: 1000, "Geometry::Body", "Mesh" {{
		Vertices: *{vertex_count} {{
			a: {vertices}
		}}
		PolygonVertexIndex: *{corner_count} {{
			a: {polygons}
		}}
		GeometryVersion: 124
	}}
	Geometry: 1001, "Geometry::Hat", "Mesh" {{
		Vertices: *24 {{
			a: {hat}
		}}
		PolygonVertexIndex: *24 {{
			a: 0,3,2,-2,4,5,6,-8,0,1,5,-5,2,3,7,-7,1,2,6,-6,0,4,7,-4
		}}
		GeometryVersion: 124
	}}
	Model: 2000, "Model::Body", "Mesh" {{
		Version: 232
	}}
	Model: 2001, "Model::Root", "LimbNode" {{
		Version: 232
	}}
	Model: 2002, "Model::Mid", "LimbNode" {{
		Version: 232
		Properties70:  {{
			P: "Lcl Translation", "Lcl Translation", "", "A",0,1,0
		}}
	}}
	Model: 2003, "Model::Top", "LimbNode" {{
		Version: 232
		Properties70:  {{
			P: "Lcl Translation", "Lcl Translation", "", "A",0,1,0
		}}
	}}
	Model: 2010, "Model::Hat", "Mesh" {{
		Version: 232
		Properties70:  {{
			P: "Lcl Translation", "Lcl Translation", "", "A",0,1.4,0
		}}
	}}
	NodeAttribute: 2101, "NodeAttribute::Root", "LimbNode" {{
		TypeFlags: "Skeleton"
	}}
	NodeAttribute: 2102, "NodeAttribute::Mid", "LimbNode" {{
		TypeFlags: "Skeleton"
	}}
	NodeAttribute: 2103, "NodeAttribute::Top", "LimbNode" {{
		TypeFlags: "Skeleton"
	}}
	Material: 2500, "Material::Green", "" {{
		Version: 102
		ShadingModel: "phong"
		Properties70:  {{
			P: "DiffuseColor", "Color", "", "A",0.2,0.7,0.3
		}}
	}}
	Deformer: 3000, "Deformer::Skin", "Skin" {{
		Version: 101
		Link_DeformAcuracy: 50
	}}
{objects}}}
Connections:  {{
	C: "OO",2000,0
	C: "OO",2001,0
	C: "OO",2002,2001
	C: "OO",2003,2002
	C: "OO",2010,2003
	C: "OO",2101,2001
	C: "OO",2102,2002
	C: "OO",2103,2003
	C: "OO",1000,2000
	C: "OO",1001,2010
	C: "OO",2500,2000
	C: "OO",2500,2010
	C: "OO",3000,1000
{connections}}}
"#,
        vertex_count = vertices.len(),
        vertices = list(&vertices),
        corner_count = faces.len() * 4,
    )
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::model::{ModelFormat, load_model, load_takes};

    fn noodle() -> crate::Model {
        load_model(
            noodle_fbx().as_bytes(),
            std::path::Path::new("."),
            ModelFormat::Fbx,
        )
        .unwrap()
    }

    /// The middle of the posed model's top ring (the column's top 4
    /// corners), in fitted space.
    fn top_middle(primitive: &[[f32; 3]]) -> Vec3 {
        let mut top: Vec<Vec3> = primitive.iter().map(|&p| Vec3::from(p)).collect();
        top.sort_by(|a, b| b.y.total_cmp(&a.y));
        top[..4].iter().sum::<Vec3>() / 4.0
    }

    #[test]
    fn the_noodle_has_a_rig_with_its_bones_and_joints() {
        let model = noodle();
        let rig = model.rig.as_ref().expect("an animated model has a rig");
        for name in ["Root", "Mid", "Top", "Hat"] {
            assert!(rig.bone(name).is_some(), "{name} in {:?}", rig.bones);
        }
        // Parents come first.
        for (index, bone) in rig.bones.iter().enumerate() {
            assert!(bone.parent.is_none_or(|parent| parent < index));
        }
        // Three skin clusters and the rigid hat.
        assert_eq!(rig.joints.len(), 4);
        for primitive in &model.primitives {
            assert_eq!(primitive.joints.len(), primitive.positions.len());
            for weights in &primitive.weights {
                let total: f32 = weights.iter().sum();
                assert!((total - 1.0).abs() < 1e-5, "{weights:?}");
            }
        }
        // 1×3×1 column plus a 0.8 hat centered 3.4 up.
        assert!((model.size[1] - 3.8).abs() < 1e-4, "{:?}", model.size);
    }

    #[test]
    fn the_rest_pose_leaves_the_model_where_it_is() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let posed = model.pose(&rig.rest_pose()).unwrap();
        for (primitive, posed) in model.primitives.iter().zip(posed.primitives(&model)) {
            for (a, b) in primitive.positions.iter().zip(&posed.positions) {
                assert!(
                    (Vec3::from(*a) - Vec3::from(*b)).length() < 1e-4,
                    "{a:?} {b:?}"
                );
            }
        }
    }

    #[test]
    fn the_takes_are_baked_with_their_names_and_lengths() {
        let takes = load_takes(noodle_fbx().as_bytes(), ModelFormat::Fbx).unwrap();
        let names: Vec<&str> = takes.iter().map(|take| take.name.as_str()).collect();
        assert_eq!(names, ["Idle", "Moves", "Walk"]);
        assert!((takes[0].duration - 2.0).abs() < 1e-3);
        assert!((takes[2].duration - 1.0).abs() < 1e-3);
        assert_eq!(takes[0].frame_rate, 30.0);
        assert_eq!(takes[0].frames(), 60);
        // Each take knows the skeleton it was made for, so it can be
        // retargeted: the scene root and the file's bones (not the rigid Hat).
        let names = |bones: &[crate::Bone]| -> Vec<String> {
            bones.iter().map(|bone| bone.name.clone()).collect()
        };
        for take in &takes {
            assert_eq!(
                names(&take.skeleton),
                ["", "Root", "Mid", "Top"],
                "{}",
                take.name
            );
            for bone in take.skeleton.iter() {
                if let Some(parent) = bone.parent {
                    assert!(parent < take.skeleton.len(), "parents come first");
                }
            }
            for track in &take.tracks {
                assert!(
                    names(&take.skeleton).contains(&track.bone),
                    "{}",
                    track.bone
                );
            }
        }
    }

    #[test]
    fn a_take_poses_the_skin_and_the_rigid_hat() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let takes = load_takes(noodle_fbx().as_bytes(), ModelFormat::Fbx).unwrap();
        let idle = &takes[0];
        let binding = idle.bind(rig);
        assert!(binding.iter().any(Option::is_some));
        let mut pose = rig.rest_pose();
        idle.sample(0.5, &binding, rig, &mut pose);
        let posed = model.pose(&pose).unwrap();
        let body = model
            .primitives
            .iter()
            .position(|p| p.positions.len() > 8)
            .unwrap();
        let hat = 1 - body;
        // Mid turned 15° around Z swings everything above it toward -X.
        let still = top_middle(&model.primitives[body].positions);
        let swayed = top_middle(&posed.primitive(&model, body, 0).positions);
        assert!(swayed.x < still.x - 0.01, "{still} -> {swayed}");
        let hat_still = top_middle(&model.primitives[hat].positions);
        let hat_swayed = top_middle(&posed.primitive(&model, hat, 0).positions);
        assert!(
            hat_swayed.x < hat_still.x - 0.01,
            "{hat_still} -> {hat_swayed}"
        );
    }

    #[test]
    fn the_walk_moves_the_root_forward() {
        let model = noodle();
        let rig = model.rig.as_ref().unwrap();
        let takes = load_takes(noodle_fbx().as_bytes(), ModelFormat::Fbx).unwrap();
        let walk = &takes[2];
        let mut pose = rig.rest_pose();
        walk.sample(0.5, &walk.bind(rig), rig, &mut pose);
        let root = rig.bone("Root").unwrap();
        let world = rig.world(&pose);
        let position = world[root].transform_point3(Vec3::ZERO);
        assert!((position.z - 1.0).abs() < 1e-3, "{position}");
    }

    #[test]
    fn the_example_project_has_this_noodle() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hello-animation/assets/models/noodle.fbx");
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if std::env::var_os("AXCENITY_WRITE_EXAMPLES").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, noodle_fbx()).unwrap();
            return;
        }
        assert!(
            on_disk == noodle_fbx(),
            "{} is out of date: run the assets tests with AXCENITY_WRITE_EXAMPLES=1",
            path.display()
        );
    }
}
