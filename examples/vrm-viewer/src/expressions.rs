//! VRM 0.x expression (blendshape) support on top of `bevy_vrm`.
//!
//! `bevy_vrm` spawns meshes with Bevy morph targets but does not wire the VRM
//! `blendShapeMaster` groups; this module parses the groups straight out of
//! the .vrm glTF JSON, resolves each bind's mesh name to the spawned
//! primitive entity (bevy_gltf tags primitives with `GltfMeshName`), and
//! pushes group values into `MorphWeights` whenever they change.

use std::collections::HashMap;
use std::path::Path;

use bevy::gltf::GltfMeshName;
use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;

/// One VRM blendshape bind: drive morph target `morph_index` on every mesh
/// primitive named `mesh_name`, scaled by `weight` of the group value.
#[derive(Clone, Debug)]
pub struct ExpressionBind {
    pub mesh_name: String,
    pub morph_index: usize,
    pub weight: f32,
}

#[derive(Clone, Debug)]
pub struct ExpressionGroup {
    /// Group name, falling back to the preset name (`joy`, `blink`, ...).
    pub name: String,
    pub is_binary: bool,
    pub binds: Vec<ExpressionBind>,
}

/// Parsed groups + current values + resolved morph bindings.
#[derive(Resource, Default)]
pub struct ExpressionRig {
    /// Model path this rig was built for ("" = nothing parsed yet).
    pub built_for: String,
    pub groups: Vec<ExpressionGroup>,
    pub values: Vec<f32>,
    /// Per group: resolved targets `(entity, morph slot, bind weight)`.
    pub bindings: Vec<Vec<(Entity, usize, f32)>>,
    resolved: bool,
    applied: Vec<f32>,
}

impl ExpressionRig {
    /// True once parsing for the current model finished (even if empty).
    pub fn parse_done(&self) -> bool {
        !self.built_for.is_empty()
    }
}

/// Extract `extensions.VRM.blendShapeMaster.blendShapeGroups` from a GLB .vrm.
pub fn parse_vrm_expressions(path: &Path) -> Option<Vec<ExpressionGroup>> {
    let bytes = std::fs::read(path).ok()?;
    // GLB layout: 12-byte header, then a JSON chunk (4-byte length,
    // "JSON" magic, data).
    if bytes.get(0..4)? != b"glTF" || bytes.get(16..20)? != b"JSON" {
        return None;
    }
    let clen = u32::from_le_bytes(bytes.get(12..16)?.try_into().ok()?) as usize;
    let end = 20usize.checked_add(clen)?;
    if bytes.len() < end {
        return None;
    }
    let doc: serde_json::Value = serde_json::from_slice(&bytes[20..end]).ok()?;

    let mesh_names: Vec<String> = doc["meshes"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .map(|m| m["name"].as_str().unwrap_or("Mesh").to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut groups = Vec::new();
    for g in doc["extensions"]["VRM"]["blendShapeMaster"]["blendShapeGroups"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = g["name"]
            .as_str()
            .or_else(|| g["presetName"].as_str())
            .unwrap_or("?")
            .to_string();
        let is_binary = g["isBinary"].as_bool().unwrap_or(false);
        let mut binds = Vec::new();
        for b in g["binds"].as_array().into_iter().flatten() {
            let (Some(mesh), Some(index)) = (b["mesh"].as_u64(), b["index"].as_u64()) else {
                continue;
            };
            // VRM 0.x exporters disagree on 0..1 vs 0..100 bind weights.
            let mut weight = b["weight"].as_f64().unwrap_or(100.0) as f32;
            if weight > 1.5 {
                weight /= 100.0;
            }
            binds.push(ExpressionBind {
                mesh_name: mesh_names.get(mesh as usize).cloned().unwrap_or_default(),
                morph_index: index as usize,
                weight,
            });
        }
        groups.push(ExpressionGroup { name, is_binary, binds });
    }
    Some(groups)
}

/// Parse on model change, then resolve binds once scene entities exist.
pub fn collect_rig(
    settings: Res<crate::Settings>,
    mut rig: ResMut<ExpressionRig>,
    named: Query<(Entity, &GltfMeshName)>,
    weights_q: Query<&MorphWeights>,
) {
    if settings.loaded.is_empty() {
        return;
    }

    // New model: reset and parse; scene entities may not exist yet.
    if rig.built_for != settings.loaded {
        let groups = parse_vrm_expressions(Path::new(&settings.loaded)).unwrap_or_default();
        let n = groups.len();
        *rig = ExpressionRig {
            built_for: settings.loaded.clone(),
            values: vec![0.0; n],
            bindings: vec![Vec::new(); n],
            groups,
            ..Default::default()
        };
        return;
    }
    if !rig.parse_done() || rig.resolved {
        return;
    }
    // Models without any blend shapes need no resolution pass.
    if rig.groups.is_empty() {
        rig.resolved = true;
        return;
    }
    // Still waiting for the glTF scene to spawn its primitives.
    if named.is_empty() || weights_q.is_empty() {
        return;
    }

    let mut by_name: HashMap<String, Vec<Entity>> = HashMap::new();
    for (entity, mesh_name) in &named {
        by_name.entry(mesh_name.0.clone()).or_default().push(entity);
    }
    // Build locally, then commit, so groups/bindings never alias.
    let mut bindings = vec![Vec::new(); rig.groups.len()];
    for (gi, group) in rig.groups.iter().enumerate() {
        for bind in &group.binds {
            for &entity in by_name.get(&bind.mesh_name).into_iter().flatten() {
                let Ok(w) = weights_q.get(entity) else { continue };
                if bind.morph_index < w.weights().len() {
                    bindings[gi].push((entity, bind.morph_index, bind.weight));
                }
            }
        }
    }
    rig.bindings = bindings;
    rig.resolved = true;
}

/// Push current group values into the morph weights (only on change).
pub fn apply_expressions(mut rig: ResMut<ExpressionRig>, mut weights_q: Query<&mut MorphWeights>) {
    if !rig.parse_done() || rig.applied == rig.values {
        return;
    }
    let mut per_entity: HashMap<Entity, Vec<(usize, f32)>> = HashMap::new();
    for (gi, _group) in rig.groups.iter().enumerate() {
        let value = rig.values[gi];
        if value == 0.0 {
            continue;
        }
        for &(entity, slot, bind_weight) in &rig.bindings[gi] {
            per_entity
                .entry(entity)
                .or_default()
                .push((slot, value * bind_weight));
        }
    }
    for (entity, contribs) in per_entity {
        let Ok(mut w) = weights_q.get_mut(entity) else { continue };
        let len = w.weights().len();
        let mut arr = vec![0.0_f32; len];
        for (slot, amount) in contribs {
            arr[slot] = (arr[slot] + amount).clamp(0.0, 1.0);
        }
        w.weights_mut().copy_from_slice(&arr);
    }
    rig.applied = rig.values.clone();
}
