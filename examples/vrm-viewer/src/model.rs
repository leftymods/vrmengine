//! Extraction of renderable geometry (vertices, indices, morph targets,
//! skinning data) from a loaded VRM model.

use glam::{Mat4, Vec3};
use vrm_engine::{LoadedModel, Vrm};

/// Maximum number of bones a single skin may use.
pub const MAX_BONES: usize = 200;

/// Vertex layout consumed by the GPU via raw byte upload (`bytes_of`), so the
/// individual fields are not read from Rust code directly.
#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joint: [f32; 4],
    pub weight: [f32; 4],
}

#[derive(Clone)]
pub struct SkinData {
    /// Runtime node index of each joint.
    pub joints: Vec<usize>,
    /// Inverse bind matrices from the glTF skin.
    pub inverse_bind: Vec<Mat4>,
    /// Per-frame `inverse_bind * world` matrices uploaded to the GPU.
    pub matrices: Vec<Mat4>,
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Base positions (rest pose, before morph targets).
    pub base_positions: Vec<[f32; 3]>,
    pub base_normals: Vec<[f32; 3]>,
    /// Per morph target, the per-vertex position delta.
    pub morph_delta_pos: Vec<Vec<[f32; 3]>>,
    /// Per morph target, the per-vertex normal delta.
    pub morph_delta_nrm: Vec<Vec<[f32; 3]>>,
    /// Runtime node index the mesh is attached to.
    pub node: usize,
    pub skin: Option<SkinData>,
    /// Material index into `doc.materials()`.
    pub material: usize,
    pub double_sided: bool,
    pub alpha: bool,
}

/// AABB of the model in rest pose, used to frame the camera.
pub struct ViewModel {
    pub meshes: Vec<MeshData>,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
}

fn read_vec3s(accessor: gltf::Accessor<'_>, buffers: &[gltf::buffer::Data]) -> Vec<[f32; 3]> {
    if let Some(iter) = gltf::accessor::Iter::<'_, [f32; 3]>::new(accessor.clone(), |b| {
        buffers.get(b.index()).map(|d| d.0.as_slice())
    }) {
        return iter.collect();
    }
    if let Some(iter) = gltf::accessor::Iter::<'_, [f32; 4]>::new(accessor, |b| {
        buffers.get(b.index()).map(|d| d.0.as_slice())
    }) {
        return iter.map(|v| [v[0], v[1], v[2]]).collect();
    }
    Vec::new()
}

/// Per-morph-target vertex deltas (positions and normals).
type MorphDeltas = (Vec<Vec<[f32; 3]>>, Vec<Vec<[f32; 3]>>);

fn morph_deltas(
    primitive: &gltf::Primitive<'_>,
    buffers: &[gltf::buffer::Data],
) -> MorphDeltas {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    for target in primitive.morph_targets() {
        if let Some(accessor) = target.positions() {
            positions.push(read_vec3s(accessor, buffers));
        }
        if let Some(accessor) = target.normals() {
            normals.push(read_vec3s(accessor, buffers));
        }
    }
    (positions, normals)
}

/// Extract renderable geometry from a loaded model.
pub fn extract(model: &LoadedModel) -> ViewModel {
    let doc = &model.vrm.doc;
    let mut meshes = Vec::new();
    let mut aabb_min = Vec3::splat(f32::INFINITY);
    let mut aabb_max = Vec3::splat(f32::NEG_INFINITY);

    for node_index in 0..model.vrm.node_count() {
        let Some(gltf_node) = doc.nodes().nth(node_index) else {
            continue;
        };
        let Some(gltf_mesh) = gltf_node.mesh() else {
            continue;
        };
        let skin = gltf_node.skin().map(|skin| {
            let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
            let joint_count = joints.len();
            let inverse_bind: Vec<Mat4> = skin
                .reader(|b| model.buffers.get(b.index()).map(|d| d.0.as_slice()))
                .read_inverse_bind_matrices()
                .map(|it| it.map(|m| Mat4::from_cols_array_2d(&m)).collect())
                .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
            SkinData {
                joints,
                inverse_bind,
                matrices: vec![Mat4::IDENTITY; joint_count],
            }
        });

        for primitive in gltf_mesh.primitives() {
            let reader = primitive.reader(|b| model.buffers.get(b.index()).map(|d| d.0.as_slice()));
            let base_positions: Vec<[f32; 3]> =
                reader.read_positions().map(|p| p.collect()).unwrap_or_default();
            let base_normals: Vec<[f32; 3]> =
                reader.read_normals().map(|n| n.collect()).unwrap_or_default();
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|t| t.into_f32().collect())
                .unwrap_or_default();
            let joints: Vec<[f32; 4]> = reader
                .read_joints(0)
                .map(|j| {
                    j.into_u16()
                        .map(|a| {
                            [
                                a[0] as f32,
                                a[1] as f32,
                                a[2] as f32,
                                a[3] as f32,
                            ]
                        })
                        .collect()
                })
                .unwrap_or_default();
            let weights: Vec<[f32; 4]> = reader
                .read_weights(0)
                .map(|w| w.into_f32().collect())
                .unwrap_or_default();
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|i| i.into_u32().collect())
                .unwrap_or_default();

            let (morph_delta_pos, morph_delta_nrm) = morph_deltas(&primitive, &model.buffers);

            let material = primitive.material();
            let double_sided = material.double_sided();
            let alpha = material.alpha_mode() != gltf::material::AlphaMode::Opaque;

            for p in &base_positions {
                let v = Vec3::from(*p);
                aabb_min = aabb_min.min(v);
                aabb_max = aabb_max.max(v);
            }

            let mut vertices = Vec::with_capacity(base_positions.len());
            for (i, &pos) in base_positions.iter().enumerate() {
                vertices.push(Vertex {
                    pos,
                    normal: base_normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                    uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
                    joint: joints.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 0.0]),
                    weight: weights.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 0.0]),
                });
            }

            meshes.push(MeshData {
                vertices,
                indices,
                base_positions,
                base_normals,
                morph_delta_pos,
                morph_delta_nrm,
                node: node_index,
                skin: skin.clone(),
                material: material.index().unwrap_or(0),
                double_sided,
                alpha,
            });
        }
    }

    if !aabb_min.is_finite() {
        aabb_min = Vec3::ZERO;
        aabb_max = Vec3::ONE;
    }
    ViewModel {
        meshes,
        aabb_min,
        aabb_max,
    }
}

impl ViewModel {
    /// Apply the current morph target weights to CPU vertex positions/normals.
    pub fn apply_morph(&mut self, vrm: &Vrm) {
        for mesh in &mut self.meshes {
            if mesh.morph_delta_pos.is_empty() {
                continue;
            }
            let weights = vrm.morph_weights(mesh.node).unwrap_or(&[]);
            let active = !weights.is_empty() && weights.iter().any(|&w| w != 0.0);
            for (i, vertex) in mesh.vertices.iter_mut().enumerate() {
                if active {
                    let mut pos = mesh.base_positions[i];
                    let mut normal = mesh.base_normals[i];
                    for (target, &w) in weights.iter().enumerate() {
                        if w == 0.0 {
                            continue;
                        }
                        let Some(delta) = mesh.morph_delta_pos.get(target) else {
                            continue;
                        };
                        pos[0] += delta[i][0] * w;
                        pos[1] += delta[i][1] * w;
                        pos[2] += delta[i][2] * w;
                        if let Some(delta) = mesh.morph_delta_nrm.get(target) {
                            normal[0] += delta[i][0] * w;
                            normal[1] += delta[i][1] * w;
                            normal[2] += delta[i][2] * w;
                        }
                    }
                    vertex.pos = pos;
                    vertex.normal = normal;
                } else {
                    vertex.pos = mesh.base_positions[i];
                    vertex.normal = mesh.base_normals[i];
                }
            }
        }
    }

    /// Recompute `inverse_bind * world` matrices for every skinned mesh.
    pub fn update_skins(&mut self, vrm: &Vrm) {
        for mesh in &mut self.meshes {
            let Some(skin) = &mut mesh.skin else {
                continue;
            };
            for (j, &joint_node) in skin.joints.iter().enumerate() {
                skin.matrices[j] = skin.inverse_bind[j] * vrm.world_matrix(joint_node);
            }
        }
    }
}
