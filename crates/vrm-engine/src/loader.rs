//! Loading of VRM files (both 0.0 and 1.0) into the [`Vrm`] runtime model.

use std::path::Path;

use glam::{Quat, Vec3};
use vrm_spec::vrm_0_0::{self, VRM};
use vrm_spec::vrmc_spring_bone_1_0::{self, VRMC_SPRING_BONE};
use vrm_spec::vrmc_vrm_1_0::{self, VRMC_VRM};

use crate::bone::BoneName;
use crate::expression::{load_expressions_vrm0, load_expressions_vrm1};
use crate::first_person::{FirstPerson, FirstPersonFlag};
use crate::humanoid::Humanoid;
use crate::look_at::{LookAtController, LookAtMode, RangeMap};
use crate::meta::{load_meta_vrm0, load_meta_vrm1, VrmMeta};
use crate::spring_bone::{Collider, SpringBoneController, SpringGroup, SpringParticle};
use crate::transform::Transform;
use crate::vrm::{Node, Vrm, VrmVersion};

/// Errors that can occur while loading a VRM model.
#[derive(Debug)]
pub enum VrmError {
    Io(std::io::Error),
    Gltf(gltf::Error),
    Json(serde_json::Error),
    /// The file is a valid glTF but has no VRM extension.
    NotVrm,
}

impl std::fmt::Display for VrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VrmError::Io(e) => write!(f, "io error: {e}"),
            VrmError::Gltf(e) => write!(f, "gltf error: {e}"),
            VrmError::Json(e) => write!(f, "json error: {e}"),
            VrmError::NotVrm => write!(f, "not a VRM model (missing VRM extension)"),
        }
    }
}

impl std::error::Error for VrmError {}

/// Load a VRM model from raw bytes (a GLB container).
pub fn load_from_bytes(bytes: &[u8]) -> Result<Vrm, VrmError> {
    let (doc, _buffers, _images) = gltf::import_slice(bytes).map_err(VrmError::Gltf)?;
    build(doc)
}

/// Load a VRM model from a file path.
pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Vrm, VrmError> {
    let bytes = std::fs::read(path).map_err(VrmError::Io)?;
    load_from_bytes(&bytes)
}

/// A loaded VRM model together with the raw glTF buffers and decoded images.
///
/// The engine itself only needs the [`Vrm`], but a renderer needs the buffer
/// data (vertices, indices, skin matrices) and decoded image pixels to draw
/// the model.
pub struct LoadedModel {
    pub vrm: Vrm,
    pub buffers: Vec<gltf::buffer::Data>,
    pub images: Vec<gltf::image::Data>,
}

/// Load a VRM model together with its glTF buffers and images from raw bytes.
pub fn load_glb_from_bytes(bytes: &[u8]) -> Result<LoadedModel, VrmError> {
    let (doc, buffers, images) = gltf::import_slice(bytes).map_err(VrmError::Gltf)?;
    let vrm = build(doc)?;
    Ok(LoadedModel { vrm, buffers, images })
}

/// Load a VRM model together with its glTF buffers and images from a file.
pub fn load_glb_from_path<P: AsRef<Path>>(path: P) -> Result<LoadedModel, VrmError> {
    let bytes = std::fs::read(path).map_err(VrmError::Io)?;
    load_glb_from_bytes(&bytes)
}

fn build(doc: gltf::Document) -> Result<Vrm, VrmError> {
    let node_count = doc.nodes().len();
    let mesh_count = doc.meshes().len();

    let version = if doc.extension_value(VRMC_VRM).is_some() {
        VrmVersion::Vrm1
    } else if doc.extension_value(VRM).is_some() {
        VrmVersion::Vrm0
    } else {
        return Err(VrmError::NotVrm);
    };

    let nodes = build_nodes(&doc);
    let order = compute_order(&nodes);
    let node_morph_count = build_node_morph_count(&doc, node_count);

    let mut vrm = Vrm {
        doc,
        version,
        meta: VrmMeta::default(),
        nodes,
        humanoid: Humanoid::new(),
        expressions: crate::expression::ExpressionManager::new(),
        look_at: None,
        spring_bones: SpringBoneController::default(),
        first_person: FirstPerson::empty(node_count, mesh_count),
        node_morph_count,
        morph_weights: Vec::new(),
        order,
    };

    vrm.update_transforms();

    match vrm.version {
        VrmVersion::Vrm1 => {
            let value = vrm
                .doc
                .extension_value(VRMC_VRM)
                .expect("version detected earlier")
                .clone();
            let schema: vrmc_vrm_1_0::VRMCVrmSchema =
                serde_json::from_value(value).map_err(VrmError::Json)?;
            vrm.humanoid = load_humanoid_vrm1(&schema);
            vrm.meta = load_meta_vrm1(&schema);
            vrm.expressions = load_expressions_vrm1(&schema, &vrm.doc);
            vrm.look_at = load_look_at_vrm1(&schema, &vrm.humanoid);
            vrm.first_person = load_first_person_vrm1(&schema, &vrm.humanoid, node_count, mesh_count);
            vrm.spring_bones = load_spring_bones_vrm1(&vrm.doc, &vrm.nodes);
        }
        VrmVersion::Vrm0 => {
            let value = vrm
                .doc
                .extension_value(VRM)
                .expect("version detected earlier")
                .clone();
            let schema: vrm_0_0::VRM0Schema =
                serde_json::from_value(value).map_err(VrmError::Json)?;
            vrm.humanoid = load_humanoid_vrm0(&schema);
            vrm.meta = load_meta_vrm0(&schema);
            vrm.expressions = load_expressions_vrm0(&schema, &vrm.doc);
            vrm.look_at = load_look_at_vrm0(&schema, &vrm.humanoid);
            vrm.first_person = load_first_person_vrm0(&schema, node_count, mesh_count);
            vrm.spring_bones = load_spring_bones_vrm0(&vrm.doc, &vrm.nodes);
        }
    }

    vrm.update_transforms();
    vrm.apply_expressions();
    Ok(vrm)
}

// ---- nodes ---------------------------------------------------------------

fn build_nodes(doc: &gltf::Document) -> Vec<Node> {
    let mut parents = vec![None; doc.nodes().len()];
    for node in doc.nodes() {
        for child in node.children() {
            parents[child.index()] = Some(node.index());
        }
    }
    doc.nodes()
        .enumerate()
        .map(|(index, node)| {
            let transform = Transform::from_gltf(node.transform());
            Node {
                index,
                name: node.name().map(str::to_string),
                parent: parents[index],
                children: node.children().map(|c| c.index()).collect(),
                initial: transform,
                local: transform,
                world: transform,
            }
        })
        .collect()
}

fn compute_order(nodes: &[Node]) -> Vec<usize> {
    let n = nodes.len();
    let mut depth = vec![0usize; n];
    for (i, _) in nodes.iter().enumerate() {
        let mut d = 0usize;
        let mut current = i;
        while let Some(parent) = nodes[current].parent {
            d += 1;
            current = parent;
            if d > n {
                d = n;
                break;
            }
        }
        depth[i] = d;
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| depth[i]);
    order
}

fn build_node_morph_count(doc: &gltf::Document, node_count: usize) -> Vec<usize> {
    (0..node_count)
        .map(|i| {
            doc.nodes()
                .nth(i)
                .and_then(|node| node.mesh())
                .map(|mesh| mesh.weights().map(|w| w.len()).unwrap_or(0))
                .unwrap_or(0)
        })
        .collect()
}

// ---- humanoid ------------------------------------------------------------

fn load_humanoid_vrm1(schema: &vrmc_vrm_1_0::VRMCVrmSchema) -> Humanoid {
    let mut humanoid = Humanoid::new();
    for (name, bone) in schema.humanoid.human_bones.0.iter() {
        if let Some(node) = bone.as_ref().and_then(|b| b.node).map(|n| n.value()) {
            humanoid.insert(BoneName::from_vrm1(*name), node);
        }
    }
    humanoid
}

fn load_humanoid_vrm0(schema: &vrm_0_0::VRM0Schema) -> Humanoid {
    let mut humanoid = Humanoid::new();
    let Some(humanoid_schema) = &schema.humanoid else {
        return humanoid;
    };
    let Some(bones) = &humanoid_schema.human_bones else {
        return humanoid;
    };
    for bone in bones {
        let Some(name) = bone.bone else {
            continue;
        };
        let Some(node) = bone.node.map(|n| n.value()) else {
            continue;
        };
        if let Some(unified) = BoneName::from_vrm0(name) {
            humanoid.insert(unified, node);
        }
    }
    humanoid
}

// ---- look at -------------------------------------------------------------

fn vec3_from_f64(v: &[f64]) -> Vec3 {
    let get = |i: usize| v.get(i).map(|&x| x as f32).unwrap_or(0.0);
    Vec3::new(get(0), get(1), get(2))
}

fn load_look_at_vrm1(
    schema: &vrmc_vrm_1_0::VRMCVrmSchema,
    humanoid: &Humanoid,
) -> Option<LookAtController> {
    let look_at = schema.look_at.as_ref()?;
    let mode = match look_at.look_at_type {
        Some(vrmc_vrm_1_0::LookAtType::Bone) => LookAtMode::Bone,
        _ => LookAtMode::Expression,
    };
    let rm = |r: &Option<vrmc_vrm_1_0::LookAtRangeMap>| RangeMap {
        input_max_value: r
            .as_ref()
            .and_then(|m| m.input_max_value)
            .map(|v| v as f32)
            .unwrap_or(10.0),
        output_scale: r
            .as_ref()
            .and_then(|m| m.output_scale)
            .map(|v| v as f32)
            .unwrap_or(1.0),
    };
    Some(LookAtController {
        mode,
        head_node: humanoid.get(BoneName::Head),
        left_eye_node: humanoid.get(BoneName::LeftEye),
        right_eye_node: humanoid.get(BoneName::RightEye),
        offset_from_head_bone: look_at
            .offset_from_head_bone
            .as_deref()
            .map(vec3_from_f64)
            .unwrap_or_default(),
        horizontal_inner: rm(&look_at.range_map_horizontal_inner),
        horizontal_outer: rm(&look_at.range_map_horizontal_outer),
        vertical_up: rm(&look_at.range_map_vertical_up),
        vertical_down: rm(&look_at.range_map_vertical_down),
    })
}

fn load_look_at_vrm0(
    schema: &vrm_0_0::VRM0Schema,
    humanoid: &Humanoid,
) -> Option<LookAtController> {
    let first_person = schema.first_person.as_ref()?;
    let mode = match first_person.look_at_type_name {
        Some(vrm_0_0::LookAtTypeName::Bone) => LookAtMode::Bone,
        _ => LookAtMode::Expression,
    };
    let offset = first_person
        .first_person_bone_offset
        .as_ref()
        .map(|o| {
            Vec3::new(
                o.x.unwrap_or(0.0) as f32,
                o.y.unwrap_or(0.0) as f32,
                o.z.unwrap_or(0.0) as f32,
            )
        })
        .unwrap_or_default();
    let rm = |m: &Option<vrm_0_0::VRMFirstPersonDegreeMap>| RangeMap {
        input_max_value: m
            .as_ref()
            .and_then(|m| m.x_range)
            .map(|v| v as f32)
            .unwrap_or(10.0),
        output_scale: m
            .as_ref()
            .and_then(|m| m.y_range)
            .map(|v| v as f32)
            .unwrap_or(1.0),
    };
    Some(LookAtController {
        mode,
        head_node: humanoid.get(BoneName::Head),
        left_eye_node: humanoid.get(BoneName::LeftEye),
        right_eye_node: humanoid.get(BoneName::RightEye),
        offset_from_head_bone: offset,
        horizontal_inner: rm(&first_person.look_at_horizontal_inner),
        horizontal_outer: rm(&first_person.look_at_horizontal_outer),
        vertical_up: rm(&first_person.look_at_vertical_up),
        vertical_down: rm(&first_person.look_at_vertical_down),
    })
}

// ---- first person --------------------------------------------------------

fn load_first_person_vrm1(
    schema: &vrmc_vrm_1_0::VRMCVrmSchema,
    humanoid: &Humanoid,
    node_count: usize,
    mesh_count: usize,
) -> FirstPerson {
    let mut fp = FirstPerson::empty(node_count, mesh_count);
    fp.bone = humanoid.get(BoneName::Head);
    if let Some(look_at) = &schema.look_at {
        if let Some(offset) = look_at.offset_from_head_bone.as_deref() {
            fp.offset = vec3_from_f64(offset);
        }
    }
    let Some(first_person) = &schema.first_person else {
        return fp;
    };
    if let Some(annotations) = &first_person.mesh_annotations {
        for annotation in annotations {
            if let Some(node) = annotation.node.map(|n| n.value()) {
                if let Some(slot) = fp.node_flags.get_mut(node) {
                    *slot = Some(FirstPersonFlag::from_vrm1(
                        annotation.mesh_annotation_type,
                    ));
                }
            }
        }
    }
    fp
}

fn load_first_person_vrm0(
    schema: &vrm_0_0::VRM0Schema,
    node_count: usize,
    mesh_count: usize,
) -> FirstPerson {
    let mut fp = FirstPerson::empty(node_count, mesh_count);
    let Some(first_person) = &schema.first_person else {
        return fp;
    };
    fp.bone = first_person.first_person_bone.map(|n| n.value());
    if let Some(offset) = &first_person.first_person_bone_offset {
        fp.offset = Vec3::new(
            offset.x.unwrap_or(0.0) as f32,
            offset.y.unwrap_or(0.0) as f32,
            offset.z.unwrap_or(0.0) as f32,
        );
    }
    if let Some(annotations) = &first_person.mesh_annotations {
        for annotation in annotations {
            let Some(mesh) = annotation.mesh.map(|m| m.value()) else {
                continue;
            };
            if let Some(flag) = annotation.first_person_flag.as_deref() {
                if let Some(slot) = fp.mesh_flags.get_mut(mesh) {
                    *slot = Some(FirstPersonFlag::from_vrm0(flag));
                }
            }
        }
    }
    fp
}

// ---- spring bones --------------------------------------------------------

fn load_spring_bones_vrm1(doc: &gltf::Document, nodes: &[Node]) -> SpringBoneController {
    let mut controller = SpringBoneController::default();
    let Some(value) = doc.extension_value(VRMC_SPRING_BONE) else {
        return controller;
    };
    let Ok(schema) =
        serde_json::from_value::<vrmc_spring_bone_1_0::VrmcSpringBoneSchema>(value.clone())
    else {
        return controller;
    };

    if let (Some(colliders), Some(collider_groups)) =
        (&schema.colliders, &schema.collider_groups)
    {
        for (group_index, group) in collider_groups.iter().enumerate() {
            for &collider_index in &group.colliders {
                let Some(collider) = colliders.get(collider_index) else {
                    continue;
                };
                let (offset, tail, radius) = if let Some(sphere) = &collider.shape.sphere {
                    (
                        sphere.offset.unwrap_or([0.0, 0.0, 0.0]),
                        None,
                        sphere.radius.unwrap_or(0.02),
                    )
                } else if let Some(capsule) = &collider.shape.capsule {
                    (
                        capsule.offset.unwrap_or([0.0, 0.0, 0.0]),
                        capsule.tail,
                        capsule.radius.unwrap_or(0.02),
                    )
                } else {
                    continue;
                };
                controller.colliders.push(Collider {
                    node: collider.node.value(),
                    offset: vec3_from_f64(&offset),
                    tail: tail.as_ref().map(|t| vec3_from_f64(t)),
                    radius: radius as f32,
                    group: group_index,
                });
            }
        }
    }

    if let Some(springs) = &schema.springs {
        for spring in springs {
            let center = spring.center.map(|c| c.value());
            let mut group = SpringGroup {
                name: spring.name.clone(),
                center,
                collider_groups: spring
                    .collider_groups
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|v| v as usize)
                    .collect(),
                stiffness: 1.0,
                drag_force: 0.4,
                gravity_dir: Vec3::new(0.0, -1.0, 0.0),
                gravity_power: 1.0,
                hit_radius: 0.02,
                particles: Vec::new(),
            };
            if let Some(first) = spring.joints.first() {
                group.stiffness = first.stiffness.map(|v| v as f32).unwrap_or(1.0);
                group.drag_force = first.drag_force.map(|v| v as f32).unwrap_or(0.4);
                group.hit_radius = first.hit_radius.map(|v| v as f32).unwrap_or(0.02);
                group.gravity_power = first.gravity_power.map(|v| v as f32).unwrap_or(1.0);
                if let Some(dir) = first.gravity_dir {
                    group.gravity_dir = vec3_from_f64(&dir);
                }
            }
            let bone_indices: Vec<usize> = spring.joints.iter().map(|j| j.node.value()).collect();
            group.particles = build_particles(&bone_indices, center, nodes);
            controller.groups.push(group);
        }
    }

    controller
}

fn load_spring_bones_vrm0(doc: &gltf::Document, nodes: &[Node]) -> SpringBoneController {
    let mut controller = SpringBoneController::default();
    let Some(value) = doc.extension_value(VRM) else {
        return controller;
    };
    let Ok(schema) = serde_json::from_value::<vrm_0_0::VRM0Schema>(value.clone()) else {
        return controller;
    };
    let Some(secondary) = &schema.secondary_animation else {
        return controller;
    };

    if let Some(collider_groups) = &secondary.collider_groups {
        for (group_index, group) in collider_groups.iter().enumerate() {
            let Some(node) = group.node.map(|n| n.value()) else {
                continue;
            };
            if let Some(colliders) = &group.colliders {
                for collider in colliders {
                    let offset = collider
                        .offset
                        .as_ref()
                        .map(|o| {
                            Vec3::new(
                                o.x.unwrap_or(0.0) as f32,
                                o.y.unwrap_or(0.0) as f32,
                                o.z.unwrap_or(0.0) as f32,
                            )
                        })
                        .unwrap_or_default();
                    let radius = collider.radius.map(|r| r as f32).unwrap_or(0.02);
                    controller.colliders.push(Collider {
                        node,
                        offset,
                        tail: None,
                        radius,
                        group: group_index,
                    });
                }
            }
        }
    }

    if let Some(bone_groups) = &secondary.bone_groups {
        for bg in bone_groups {
            let Some(bones) = &bg.bones else {
                continue;
            };
            let center = bg.center.map(|n| n.value());
            let gravity_dir = bg
                .gravity_dir
                .as_ref()
                .map(|g| {
                    Vec3::new(
                        g.x.unwrap_or(0.0) as f32,
                        g.y.unwrap_or(0.0) as f32,
                        g.z.unwrap_or(0.0) as f32,
                    )
                })
                .unwrap_or_else(|| Vec3::new(0.0, -1.0, 0.0));
            let mut group = SpringGroup {
                name: bg.comment.clone(),
                center,
                collider_groups: bg
                    .collider_groups
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|v| v as usize)
                    .collect(),
                stiffness: bg.stiffiness.map(|v| v as f32).unwrap_or(1.0),
                drag_force: bg.drag_force.map(|v| v as f32).unwrap_or(0.4),
                gravity_dir,
                gravity_power: bg.gravity_power.map(|v| v as f32).unwrap_or(1.0),
                hit_radius: bg.hit_radius.map(|v| v as f32).unwrap_or(0.02),
                particles: Vec::new(),
            };
            let bone_indices: Vec<usize> = bones.iter().map(|b| b.value()).collect();
            group.particles = build_particles(&bone_indices, center, nodes);
            controller.groups.push(group);
        }
    }

    controller
}

fn build_particles(bone_indices: &[usize], center: Option<usize>, nodes: &[Node]) -> Vec<SpringParticle> {
    let mut particles = Vec::with_capacity(bone_indices.len());
    for (i, &node_index) in bone_indices.iter().enumerate() {
        let verlet_parent = if i == 0 {
            center.or_else(|| nodes.get(node_index).and_then(|n| n.parent))
        } else {
            Some(bone_indices[i - 1])
        };
        let scene_parent = nodes.get(node_index).and_then(|n| n.parent);
        let parent_pos = verlet_parent
            .map(|p| nodes[p].world.translation)
            .unwrap_or(Vec3::ZERO);
        let parent_rot = verlet_parent
            .map(|p| nodes[p].world.rotation)
            .unwrap_or(Quat::IDENTITY);
        let world_pos = nodes[node_index].world.translation;
        let local_offset = parent_rot.inverse() * (world_pos - parent_pos);
        let rest_len = local_offset.length();
        let rest_world_dir = if rest_len > 1e-9 {
            (world_pos - parent_pos) / rest_len
        } else {
            Vec3::Z
        };
        particles.push(SpringParticle {
            node: node_index,
            verlet_parent,
            scene_parent,
            local_offset,
            rest_len,
            rest_world_dir,
            initial_world_rot: nodes[node_index].world.rotation,
            prev: world_pos,
            current: world_pos,
        });
    }
    particles
}
