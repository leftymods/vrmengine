//! The [`Vrm`] runtime model: scene graph, humanoid, expressions, look-at and
//! spring bones.

use glam::{Mat4, Vec3};

use crate::bone::BoneName;
use crate::expression::{ExpressionId, ExpressionManager};
use crate::first_person::{FirstPerson, FirstPersonCamera, FirstPersonFlag};
use crate::humanoid::Humanoid;
use crate::look_at::{LookAtController, LookAtMode};
use crate::spring_bone::SpringBoneController;
use crate::transform::Transform;

/// VRM specification version of a loaded model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrmVersion {
    Vrm0,
    Vrm1,
}

impl std::fmt::Display for VrmVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VrmVersion::Vrm0 => f.write_str("0.0"),
            VrmVersion::Vrm1 => f.write_str("1.0"),
        }
    }
}

/// A runtime scene graph node.
#[derive(Debug, Clone)]
pub struct Node {
    pub index: usize,
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// The transform in the rest pose.
    pub initial: Transform,
    /// The current animated local transform.
    pub local: Transform,
    /// The current world transform.
    pub world: Transform,
}

impl Node {
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    pub fn world_matrix(&self) -> Mat4 {
        self.world.to_mat4()
    }

    pub fn reset(&mut self) {
        self.local = self.initial;
        self.world = self.initial;
    }
}

/// The runtime representation of a VRM model.
pub struct Vrm {
    /// The parsed glTF document. Use it to access meshes, skins, materials,
    /// cameras, etc.
    pub doc: gltf::Document,
    pub version: VrmVersion,
    pub meta: crate::meta::VrmMeta,
    pub nodes: Vec<Node>,
    pub humanoid: Humanoid,
    pub expressions: ExpressionManager,
    pub look_at: Option<LookAtController>,
    pub spring_bones: SpringBoneController,
    pub first_person: FirstPerson,

    pub(crate) node_morph_count: Vec<usize>,
    pub(crate) morph_weights: Vec<Vec<f32>>,
    pub(crate) order: Vec<usize>,
}

impl Vrm {
    pub fn node(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    pub fn node_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.nodes.get_mut(index)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn set_node_local_transform(&mut self, index: usize, transform: Transform) {
        if let Some(node) = self.nodes.get_mut(index) {
            node.local = transform;
        }
    }

    /// Recompute all world transforms from local transforms.
    pub fn update_transforms(&mut self) {
        let order = self.order.clone();
        for index in order {
            let parent_world = self.nodes[index]
                .parent
                .and_then(|p| self.nodes.get(p))
                .map(|p| p.world)
                .unwrap_or_else(Transform::identity);
            let local = self.nodes[index].local;
            self.nodes[index].world = parent_world.mul(&local);
        }
    }

    /// World transform of a node.
    pub fn world_transform(&self, index: usize) -> Transform {
        self.nodes
            .get(index)
            .map(|n| n.world)
            .unwrap_or_else(Transform::identity)
    }

    /// World matrix of a node.
    pub fn world_matrix(&self, index: usize) -> Mat4 {
        self.world_transform(index).to_mat4()
    }

    /// The runtime node index bound to a humanoid bone.
    pub fn human_bone(&self, bone: BoneName) -> Option<usize> {
        self.humanoid.get(bone)
    }

    /// Reset all nodes, expression weights and spring bone state to the rest
    /// pose.
    pub fn reset_pose(&mut self) {
        for node in &mut self.nodes {
            node.reset();
        }
        self.expressions.reset();
        self.apply_expressions();
        self.spring_bones.reset(&self.nodes);
        self.update_transforms();
    }

    // ---- expressions ------------------------------------------------------

    pub fn set_expression(&mut self, id: &ExpressionId, weight: f32) {
        self.expressions.set_weight(id, weight);
    }

    pub fn expression_weight(&self, id: &ExpressionId) -> f32 {
        self.expressions.weight(id)
    }

    /// Reset all expression weights to 0 and recompute morph weights.
    pub fn reset_expressions(&mut self) {
        self.expressions.reset();
        self.apply_expressions();
    }

    /// Recompute per-node morph target weights from the current expression
    /// weights.
    pub fn apply_expressions(&mut self) {
        self.morph_weights = self.expressions.compute_morph_weights(&self.node_morph_count);
    }

    /// The accumulated morph target weights of the mesh attached to `node`.
    pub fn morph_weights(&self, node: usize) -> Option<&[f32]> {
        self.morph_weights.get(node).map(|v| v.as_slice())
    }

    // ---- look at ----------------------------------------------------------

    /// Set the world-space target the eyes should look at.
    ///
    /// In bone mode the eye bones are rotated; in expression mode the
    /// look-direction expression weights are updated.
    pub fn update_look_at(&mut self, target: Vec3) {
        let Some(controller) = self.look_at.clone() else {
            return;
        };
        let result = controller.evaluate(target, &self.nodes);

        match controller.mode {
            LookAtMode::Bone => {
                if let (Some(left_eye), Some(left_rotation)) =
                    (controller.left_eye_node, result.left_eye_rotation)
                {
                    if let Some(node) = self.nodes.get_mut(left_eye) {
                        node.local.rotation = node.initial.rotation * left_rotation;
                    }
                }
                if let (Some(right_eye), Some(right_rotation)) =
                    (controller.right_eye_node, result.right_eye_rotation)
                {
                    if let Some(node) = self.nodes.get_mut(right_eye) {
                        node.local.rotation = node.initial.rotation * right_rotation;
                    }
                }
            }
            LookAtMode::Expression => {
                for (preset, weight) in result.expression_weights {
                    self.expressions.set_weight(&ExpressionId::Preset(preset), weight);
                }
                self.apply_expressions();
            }
        }
    }

    // ---- spring bones -----------------------------------------------------

    /// Advance the spring bone simulation by `dt` seconds.
    pub fn update_spring_bones(&mut self, dt: f32) {
        if self.spring_bones.is_empty() {
            return;
        }
        self.update_transforms();
        self.spring_bones.update(&mut self.nodes, dt);
        self.update_transforms();
    }

    // ---- first person -----------------------------------------------------

    /// Whether a mesh should be rendered for the given camera.
    pub fn is_mesh_visible(&self, mesh_index: usize, camera: FirstPersonCamera) -> bool {
        let flag = self.flag_for_mesh(mesh_index);
        match flag {
            FirstPersonFlag::Both => true,
            FirstPersonFlag::FirstPersonOnly => camera == FirstPersonCamera::FirstPerson,
            FirstPersonFlag::ThirdPersonOnly => camera == FirstPersonCamera::ThirdPerson,
            FirstPersonFlag::Auto => {
                if camera == FirstPersonCamera::ThirdPerson {
                    return true;
                }
                // In first person, hide meshes attached to the head subtree.
                let in_head_subtree = match self.first_person.bone {
                    Some(bone) => self
                        .nodes_for_mesh(mesh_index)
                        .any(|n| self.is_node_descendant(n, bone)),
                    None => false,
                };
                !in_head_subtree
            }
        }
    }

    /// The resolved first person flag for a mesh.
    pub fn mesh_first_person_flag(&self, mesh_index: usize) -> FirstPersonFlag {
        self.flag_for_mesh(mesh_index)
    }

    fn flag_for_mesh(&self, mesh_index: usize) -> FirstPersonFlag {
        if let Some(flag) = self
            .first_person
            .mesh_flags
            .get(mesh_index)
            .copied()
            .flatten()
        {
            return flag;
        }
        for node in self.nodes_for_mesh(mesh_index) {
            if let Some(flag) = self.first_person.node_flags.get(node).copied().flatten() {
                return flag;
            }
        }
        FirstPersonFlag::Auto
    }

    fn nodes_for_mesh(&self, mesh_index: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes.iter().filter_map(move |node| {
            self.doc
                .nodes()
                .nth(node.index)
                .and_then(|n| n.mesh())
                .filter(|m| m.index() == mesh_index)
                .map(|_| node.index)
        })
    }

    fn is_node_descendant(&self, node: usize, ancestor: usize) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.get(current).and_then(|n| n.parent) {
            if parent == ancestor {
                return true;
            }
            current = parent;
        }
        false
    }
}
