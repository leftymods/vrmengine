//! VRM first-person rendering support, ported from `@pixiv/three-vrm-core/firstPerson`.
//!
//! Models whether each mesh should be visible from the first-person camera. `auto` meshes are
//! split into a "headless" copy for the first-person view (triangles influenced by head bones are
//! removed) and the full mesh for the third-person view.

use crate::scene::{Mesh, Node, Primitive, Scene};

/// How a mesh is annotated for first-person rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstPersonFlag {
    Auto,
    Both,
    ThirdPersonOnly,
    FirstPersonOnly,
}

impl FirstPersonFlag {
    pub fn from_v1(s: &str) -> Self {
        match s {
            "both" => FirstPersonFlag::Both,
            "thirdPersonOnly" => FirstPersonFlag::ThirdPersonOnly,
            "firstPersonOnly" => FirstPersonFlag::FirstPersonOnly,
            _ => FirstPersonFlag::Auto,
        }
    }

    /// VRM 0.x `firstPersonFlag` -> VRM 1.0 type (default `Auto`).
    pub fn from_v0(s: Option<&str>) -> Self {
        match s {
            Some("FirstPersonOnly") => FirstPersonFlag::FirstPersonOnly,
            Some("ThirdPersonOnly") => FirstPersonFlag::ThirdPersonOnly,
            Some("Both") => FirstPersonFlag::Both,
            _ => FirstPersonFlag::Auto,
        }
    }
}

/// Annotation of a mesh node (three.js `VRMFirstPersonMeshAnnotation`).
#[derive(Debug, Clone, Copy)]
pub struct MeshAnnotation {
    pub node: usize,
    pub flag: FirstPersonFlag,
}

/// The first-person rendering component of a VRM (three.js `VRMFirstPerson`).
#[derive(Debug, Clone, Default)]
pub struct FirstPerson {
    pub head_bone: Option<usize>,
    pub mesh_annotations: Vec<MeshAnnotation>,
    /// Nodes visible from the first-person camera, computed by `setup`.
    pub first_person_nodes: Vec<usize>,
    /// Nodes visible from the third-person camera, computed by `setup`.
    pub third_person_nodes: Vec<usize>,
}

impl FirstPerson {
    pub fn new(head_bone: Option<usize>, mesh_annotations: Vec<MeshAnnotation>) -> Self {
        FirstPerson {
            head_bone,
            mesh_annotations,
            first_person_nodes: Vec::new(),
            third_person_nodes: Vec::new(),
        }
    }

    pub fn get_first_person_mesh_annotation(&self, node: usize) -> Option<&MeshAnnotation> {
        self.mesh_annotations.iter().find(|a| a.node == node)
    }

    pub fn set_first_person_flag(&mut self, node: usize, flag: FirstPersonFlag) {
        if let Some(annotation) = self.mesh_annotations.iter_mut().find(|a| a.node == node) {
            annotation.flag = flag;
        }
    }

    pub fn get_first_person_bone_node(&self) -> Option<usize> {
        self.head_bone
    }

    /// Is the given node the head bone or a descendant of it?
    fn is_head_related(&self, scene: &Scene, node: usize) -> bool {
        match self.head_bone {
            Some(head) => scene.is_descendant_of(node, head),
            None => false,
        }
    }

    /// Assign visibility lists and generate headless copies for `auto` meshes.
    ///
    /// This is an equivalent of `VRMFirstPerson.setup` adapted to this engine's index-based
    /// scene graph. The generated headless meshes are appended to the scene; the returned value
    /// is the list of mesh indices appended (in order).
    pub fn setup(&mut self, scene: &mut Scene) -> Vec<usize> {
        self.first_person_nodes.clear();
        self.third_person_nodes.clear();
        let mut appended: Vec<usize> = Vec::new();

        let mut mesh_nodes: Vec<usize> = scene
            .nodes
            .iter()
            .filter(|n| n.mesh.is_some())
            .map(|n| n.index)
            .collect();
        mesh_nodes.sort_unstable();

        for node_index in mesh_nodes {
            let annotation = self.get_first_person_mesh_annotation(node_index);
            let flag = annotation.map(|a| a.flag).unwrap_or(FirstPersonFlag::Auto);

            match flag {
                FirstPersonFlag::FirstPersonOnly => {
                    self.first_person_nodes.push(node_index);
                }
                FirstPersonFlag::ThirdPersonOnly => {
                    self.third_person_nodes.push(node_index);
                }
                FirstPersonFlag::Both => {
                    self.first_person_nodes.push(node_index);
                    self.third_person_nodes.push(node_index);
                }
                FirstPersonFlag::Auto => {
                    let clone = self.create_headless_mesh(scene, node_index);
                    match clone {
                        Some(mesh_index) => {
                            // the full mesh stays visible in third person, the headless copy in first person
                            self.third_person_nodes.push(node_index);
                            appended.push(mesh_index);
                            if let Some(node) = scene.nodes.last_mut() {
                                self.first_person_nodes.push(node.index);
                            }
                        }
                        None => {
                            // no head-related bones: visible from both cameras
                            self.first_person_nodes.push(node_index);
                            self.third_person_nodes.push(node_index);
                        }
                    }
                }
            }
        }

        appended
    }

    /// Build a headless copy of the mesh referenced by `node_index`.
    ///
    /// Returns the index of the appended mesh, or `None` if the mesh has no triangles influenced
    /// by head-related bones. Mirrors `_createErasedMesh`.
    fn create_headless_mesh(&self, scene: &mut Scene, node_index: usize) -> Option<usize> {
        // Extract everything we need from the node up front so we don't hold an immutable
        // borrow of `scene` while we later mutate `scene.meshes` / `scene.nodes`.
        let (mesh_index, skin_index, node_name, parent, translation, rotation, scale) = {
            let node = scene.node(node_index);
            (
                node.mesh?,
                node.skin?,
                node.name.clone(),
                node.parent,
                node.translation,
                node.rotation,
                node.scale,
            )
        };

        // indices of the skin joints that are the head bone or its descendants
        let erase_bone_indexes: Vec<usize> = scene.skins[skin_index]
            .joints
            .iter()
            .enumerate()
            .filter(|(_, joint)| self.is_head_related(scene, **joint))
            .map(|(i, _)| i)
            .collect();

        if erase_bone_indexes.is_empty() {
            return None;
        }

        let source = scene.meshes[mesh_index].clone();
        let mut primitives: Vec<Primitive> = Vec::with_capacity(source.primitives.len());

        for primitive in &source.primitives {
            let mut clone = primitive.clone();
            clone.indices = filter_head_influenced_indices(primitive, &erase_bone_indexes);
            if clone.indices.as_ref().map(|i| i.is_empty()).unwrap_or(true) {
                continue;
            }
            primitives.push(clone);
        }

        if primitives.is_empty() {
            return None;
        }

        let mesh_name = format!("{}_headless", source.name);
        let mesh = Mesh {
            name: mesh_name,
            primitives,
        };
        let mesh_index = scene.meshes.len();
        scene.meshes.push(mesh);

        // a new node bound to the same skin, placed at the original node's local transform
        let node_index_new = scene.nodes.len();
        let mut clone_node = Node::new(node_index_new);
        clone_node.name = format!("{}_headless", node_name);
        clone_node.parent = parent;
        clone_node.translation = translation;
        clone_node.rotation = rotation;
        clone_node.scale = scale;
        clone_node.mesh = Some(mesh_index);
        clone_node.skin = Some(skin_index);
        clone_node.update_matrix();

        if let Some(parent) = parent {
            scene.nodes[parent].children.push(node_index_new);
        } else {
            scene.root_nodes.push(node_index_new);
        }
        scene.nodes.push(clone_node);

        Some(mesh_index)
    }
}

/// Filter out triangles that are influenced by any of the given bone indices.
fn filter_head_influenced_indices(primitive: &Primitive, erase_bone_indexes: &[usize]) -> Option<Vec<u32>> {
    let indices = primitive.indices.as_ref()?;
    let joints = primitive.joints.as_ref()?;
    let weights = primitive.weights.as_ref()?;

    let mut out: Vec<u32> = Vec::with_capacity(indices.len());
    for tri in indices.chunks_exact(3) {
        let mut skip = false;
        for &v in tri {
            let v = v as usize;
            let joints_v = &joints[v];
            let weights_v = &weights[v];
            for j in 0..4 {
                if weights_v[j] > 0.0 && erase_bone_indexes.contains(&(joints_v[j] as usize)) {
                    skip = true;
                    break;
                }
            }
            if skip {
                break;
            }
        }
        if !skip {
            out.extend_from_slice(tri);
        }
    }

    Some(out)
}
