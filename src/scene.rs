//! A minimal scene graph, roughly equivalent to the subset of three.js used by three-vrm.
//!
//! Nodes hold TRS transforms and world matrices. Meshes/primitives hold geometry (including
//! morph targets), skins hold inverse-bind matrices, textures/images hold decoded image data.

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::material::Material;

pub const IDENTITY: Mat4 = Mat4::IDENTITY;

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub index: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub local_matrix: Mat4,
    pub world_matrix: Mat4,
    /// three.js `matrixAutoUpdate`. Disabled for spring bone bones.
    pub matrix_auto_update: bool,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub camera: Option<usize>,
    pub visible: bool,
}

impl Node {
    pub fn new(index: usize) -> Self {
        Node {
            name: String::new(),
            index,
            parent: None,
            children: Vec::new(),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            local_matrix: Mat4::IDENTITY,
            world_matrix: Mat4::IDENTITY,
            matrix_auto_update: true,
            mesh: None,
            skin: None,
            camera: None,
            visible: true,
        }
    }

    pub fn update_matrix(&mut self) {
        self.local_matrix = Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation);
    }
}

#[derive(Debug, Clone)]
pub struct MorphTarget {
    pub positions: Option<Vec<[f32; 3]>>,
    pub normals: Option<Vec<[f32; 3]>>,
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub texcoords: Option<Vec<[f32; 2]>>,
    pub colors: Option<Vec<[f32; 4]>>,
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
    pub indices: Option<Vec<u32>>,
    pub morph_targets: Vec<MorphTarget>,
    /// Runtime morph target influences; mutated by expression binds.
    pub morph_weights: Vec<f32>,
    pub material: Option<usize>,
    pub mode: u32,
}

impl Primitive {
    /// Number of vertices in this primitive.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
}

#[derive(Debug, Clone)]
pub struct Skin {
    pub name: String,
    /// Joint node indices.
    pub joints: Vec<usize>,
    /// Inverse bind matrices, in the same order as `joints`.
    pub inverse_bind_matrices: Vec<Mat4>,
    pub skeleton: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ImageData {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// RGBA8, top row first (as decoded from the source image).
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    Repeat,
    MirroredRepeat,
    ClampToEdge,
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub image: usize,
    pub wrap_s: WrapMode,
    pub wrap_t: WrapMode,
    pub mag_filter: Option<u32>,
    pub min_filter: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct PerspectiveCamera {
    pub fovy: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone)]
pub struct GltfAnimationSampler {
    pub input: Vec<f32>,
    pub output: Vec<f32>,
    pub interpolation: String,
    /// Per-keyframe component count.
    pub component_count: usize,
}

#[derive(Debug, Clone)]
pub struct GltfAnimationChannel {
    pub sampler: usize,
    pub node: usize,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct GltfAnimation {
    pub name: String,
    pub samplers: Vec<GltfAnimationSampler>,
    pub channels: Vec<GltfAnimationChannel>,
    pub duration: f32,
}

#[derive(Debug, Default)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub skins: Vec<Skin>,
    pub images: Vec<ImageData>,
    pub textures: Vec<Texture>,
    pub materials: Vec<Material>,
    pub cameras: Vec<Option<PerspectiveCamera>>,
    pub animations: Vec<GltfAnimation>,
    pub root_nodes: Vec<usize>,
}

impl Scene {
    pub fn node(&self, index: usize) -> &Node {
        &self.nodes[index]
    }

    pub fn node_mut(&mut self, index: usize) -> &mut Node {
        &mut self.nodes[index]
    }

    /// three.js `Object3D.getWorldPosition`.
    pub fn node_world_position(&self, index: usize) -> Vec3 {
        self.nodes[index].world_matrix.to_scale_rotation_translation().2
    }

    /// three.js `getWorldQuaternionLite`.
    pub fn node_world_quaternion(&self, index: usize) -> Quat {
        let (_, quat, _) = self.nodes[index].world_matrix.to_scale_rotation_translation();
        quat
    }

    /// three.js `Object3D.getWorldQuaternion` with matrix update.
    pub fn node_world_quaternion_deep(&mut self, index: usize) -> Quat {
        self.update_world_matrix(index, true, false);
        self.node_world_quaternion(index)
    }

    /// Update local matrix of a node from its TRS.
    pub fn node_update_matrix(&mut self, index: usize) {
        let node = &mut self.nodes[index];
        node.local_matrix = Mat4::from_scale_rotation_translation(node.scale, node.rotation, node.translation);
    }

    /// Recompute a node's world matrix as `parent.world * local`.
    pub fn node_update_world(&mut self, index: usize) {
        let parent_world = self.nodes[index].parent.map(|p| self.nodes[p].world_matrix);
        let node = &mut self.nodes[index];
        node.world_matrix = parent_world.unwrap_or(IDENTITY) * node.local_matrix;
    }

    /// Recursively update the world matrix of `index` and all its descendants.
    pub fn update_world_matrix(&mut self, index: usize, update_parents: bool, update_children: bool) {
        if update_parents {
            self.update_world_matrix_parents(index);
        }
        let parent_world = self.nodes[index].parent.map(|p| self.nodes[p].world_matrix);
        let node = &mut self.nodes[index];
        node.world_matrix = parent_world.unwrap_or(IDENTITY) * node.local_matrix;
        if update_children {
            let children = node.children.clone();
            for child in children {
                self.update_world_matrix(child, false, true);
            }
        }
    }

    fn update_world_matrix_parents(&mut self, index: usize) {
        let mut chain: Vec<usize> = Vec::new();
        let mut cur = self.nodes[index].parent;
        while let Some(i) = cur {
            chain.push(i);
            cur = self.nodes[i].parent;
        }
        chain.reverse();
        for i in chain {
            let parent_world = self.nodes[i].parent.map(|p| self.nodes[p].world_matrix);
            let node = &mut self.nodes[i];
            node.world_matrix = parent_world.unwrap_or(IDENTITY) * node.local_matrix;
        }
    }

    /// Update world matrices of every node that has `matrix_auto_update == true`.
    /// Nodes with `matrix_auto_update == false` (spring bones) keep their manually-updated matrix,
    /// but their children still get recomputed from the (possibly stale) parent matrix.
    pub fn update_world_matrices(&mut self) {
        for &root in self.root_nodes.clone().iter() {
            self.update_world_matrices_dfs(root, None);
        }
    }

    fn update_world_matrices_dfs(&mut self, index: usize, parent_world: Option<Mat4>) {
        let children = self.nodes[index].children.clone();
        {
            let node = &mut self.nodes[index];
            if node.matrix_auto_update {
                node.world_matrix = parent_world.unwrap_or(IDENTITY) * node.local_matrix;
            }
            let world = node.world_matrix;
            for child in children {
                self.update_world_matrices_dfs(child, Some(world));
            }
        }
    }

    /// Find the root ancestor of a node.
    pub fn root_of(&self, index: usize) -> usize {
        let mut cur = index;
        while let Some(p) = self.nodes[cur].parent {
            cur = p;
        }
        cur
    }

    /// Collect the ordered ancestor chain from root down to (not including) `index`.
    pub fn ancestor_chain(&self, index: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut cur = self.nodes[index].parent;
        while let Some(i) = cur {
            chain.push(i);
            cur = self.nodes[i].parent;
        }
        chain.reverse();
        chain
    }

    /// True if `descendant` is the same as or a descendant of `ancestor`.
    pub fn is_descendant_of(&self, descendant: usize, ancestor: usize) -> bool {
        let mut cur = Some(descendant);
        while let Some(i) = cur {
            if i == ancestor {
                return true;
            }
            cur = self.nodes[i].parent;
        }
        false
    }

    /// Gather all node indices in `index`'s subtree (including itself), depth-first.
    pub fn subtree(&self, index: usize) -> Vec<usize> {
        let mut out = Vec::new();
        self.subtree_dfs(index, &mut out);
        out
    }

    fn subtree_dfs(&self, index: usize, out: &mut Vec<usize>) {
        out.push(index);
        for child in &self.nodes[index].children {
            self.subtree_dfs(*child, out);
        }
    }

    /// Gather all node indices that render (have a mesh) in the given subtree, with their world matrices.
    pub fn visible_mesh_nodes(&self) -> Vec<(usize, &Node)> {
        let mut out = Vec::new();
        for root in &self.root_nodes {
            self.visible_mesh_nodes_dfs(*root, &mut out);
        }
        out
    }

    fn visible_mesh_nodes_dfs<'a>(&'a self, index: usize, out: &mut Vec<(usize, &'a Node)>) {
        let node = &self.nodes[index];
        if !node.visible {
            return;
        }
        if node.mesh.is_some() {
            out.push((index, node));
        }
        for child in &node.children {
            self.visible_mesh_nodes_dfs(*child, out);
        }
    }

    /// Compute morphed vertex data for a primitive.
    /// Returns (positions, normals) after applying `morph_weights`.
    pub fn compute_morph(&self, primitive: &Primitive) -> (Vec<f32>, Option<Vec<f32>>) {
        let base = &primitive.positions;
        let n = base.len();
        let mut positions = Vec::with_capacity(n * 3);
        positions.extend(base.iter().flatten().copied());

        let mut normals = primitive.normals.as_ref().map(|norm| {
            let mut v = Vec::with_capacity(norm.len() * 3);
            v.extend(norm.iter().flatten().copied());
            v
        });

        for (mi, target) in primitive.morph_targets.iter().enumerate() {
            let weight = primitive.morph_weights.get(mi).copied().unwrap_or(0.0);
            if weight.abs() < 1e-6 {
                continue;
            }
            if let Some(delta_pos) = &target.positions {
                for i in 0..n {
                    positions[i * 3] += delta_pos[i][0] * weight;
                    positions[i * 3 + 1] += delta_pos[i][1] * weight;
                    positions[i * 3 + 2] += delta_pos[i][2] * weight;
                }
            }
            if let (Some(delta_norm), Some(norm)) = (&target.normals, &mut normals) {
                for i in 0..n {
                    norm[i * 3] += delta_norm[i][0] * weight;
                    norm[i * 3 + 1] += delta_norm[i][1] * weight;
                    norm[i * 3 + 2] += delta_norm[i][2] * weight;
                }
            }
        }

        if let Some(norm) = &mut normals {
            for i in 0..n {
                let v = Vec3::new(norm[i * 3], norm[i * 3 + 1], norm[i * 3 + 2]);
                let v = v.normalize_or_zero();
                norm[i * 3] = v.x;
                norm[i * 3 + 1] = v.y;
                norm[i * 3 + 2] = v.z;
            }
        }

        (positions, normals)
    }

    /// Compute skinned + morphed vertex data on the CPU. Used as fallback when the bone count
    /// exceeds the shader uniform limit.
    pub fn compute_skinned(
        &self,
        primitive: &Primitive,
        bone_matrices: &[Mat4],
    ) -> (Vec<f32>, Option<Vec<f32>>) {
        let (positions, normals) = self.compute_morph(primitive);
        let n = primitive.vertex_count();
        let joints = primitive.joints.as_ref().unwrap();
        let weights = primitive.weights.as_ref().unwrap();

        let mut skinned_pos = vec![0.0f32; n * 3];
        let mut skinned_norm = normals.as_ref().map(|norm| vec![0.0f32; norm.len()]);

        for i in 0..n {
            let pos = Vec4::new(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2], 1.0);
            let norm = skinned_norm.as_ref().map(|_| {
                Vec3::new(
                    normals.as_ref().unwrap()[i * 3],
                    normals.as_ref().unwrap()[i * 3 + 1],
                    normals.as_ref().unwrap()[i * 3 + 2],
                )
            });

            let mut pos_acc = Vec4::ZERO;
            let mut norm_acc = Vec3::ZERO;
            for j in 0..4 {
                let w = weights[i][j];
                if w == 0.0 {
                    continue;
                }
                let joint = joints[i][j] as usize;
                let mat = bone_matrices.get(joint).copied().unwrap_or(IDENTITY);
                pos_acc += mat * pos * w;
                if let (Some(n), Some(_)) = (norm, &skinned_norm) {
                    norm_acc += mat.transform_vector3(n) * w;
                }
            }

            skinned_pos[i * 3] = pos_acc.x;
            skinned_pos[i * 3 + 1] = pos_acc.y;
            skinned_pos[i * 3 + 2] = pos_acc.z;
            if let Some(sn) = &mut skinned_norm {
                let v = norm_acc.normalize_or_zero();
                sn[i * 3] = v.x;
                sn[i * 3 + 1] = v.y;
                sn[i * 3 + 2] = v.z;
            }
        }

        (skinned_pos, skinned_norm)
    }

    /// Build the bone matrices (joint world * inverse bind) for a skin.
    pub fn bone_matrices(&self, skin: usize) -> Vec<Mat4> {
        let skin = &self.skins[skin];
        skin.joints
            .iter()
            .zip(skin.inverse_bind_matrices.iter())
            .map(|(joint, inv)| self.nodes[*joint].world_matrix * *inv)
            .collect()
    }

    /// Find all node indices that reference the given mesh.
    pub fn nodes_using_mesh(&self, mesh_index: usize) -> Vec<usize> {
        self.nodes
            .iter()
            .filter(|n| n.mesh == Some(mesh_index))
            .map(|n| n.index)
            .collect()
    }

    /// Map from gltf node index to the meshes' primitive material indices. Kept for v0 material binds.
    pub fn node_mesh_material_map(&self) -> HashMap<usize, Vec<Option<usize>>> {
        let mut map = HashMap::new();
        for node in &self.nodes {
            if let Some(mesh_index) = node.mesh {
                if let Some(mesh) = self.meshes.get(mesh_index) {
                    map.insert(
                        node.index,
                        mesh.primitives.iter().map(|p| p.material).collect(),
                    );
                }
            }
        }
        map
    }

    /// Get the material indices used by a gltf mesh (one per primitive).
    pub fn mesh_material_indices(&self, mesh_index: usize) -> Vec<Option<usize>> {
        self.meshes
            .get(mesh_index)
            .map(|m| m.primitives.iter().map(|p| p.material).collect())
            .unwrap_or_default()
    }

    pub fn node_by_name(&self, name: &str) -> Option<usize> {
        self.nodes.iter().find(|n| n.name == name).map(|n| n.index)
    }

    pub fn root_world(&mut self) -> Mat4 {
        self.update_world_matrices();
        let root = self.root_nodes[0];
        self.nodes[root].world_matrix
    }
}
