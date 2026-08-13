//! GLB / glTF 2.0 loader, ported to use the `gltf` 1.4 crate API.
//!
//! Parses the binary glTF container, builds our `Scene`, and exposes the raw extension JSON so the
//! VRM loader can consume `VRMC_vrm`, `VRM` (0.x), `VRMC_springBone`, `VRMC_materials_mtoon`, etc.

use std::path::Path;

use anyhow::{bail, Context, Result};
use base64::Engine;
use glam::{Mat4, Quat, Vec3, Vec4};

use crate::material::{AlphaMode, Material, MaterialKind, OutlineWidthMode};
use crate::scene::{
    GltfAnimation, GltfAnimationChannel, GltfAnimationSampler, ImageData, Mesh, MorphTarget, Node,
    PerspectiveCamera, Primitive, Scene, Skin, Texture, WrapMode,
};

pub struct LoadedGltf {
    pub scene: Scene,
    /// Raw extension JSON of the root glTF object, e.g. `extensions.VRMC_vrm`.
    pub root_extensions: serde_json::Map<String, serde_json::Value>,
    /// `extensionsUsed` array.
    pub extensions_used: Vec<String>,
}

/// gamma EOTF used by the VRM 0.x compat plugin (ported from `utils/gammaEOTF.ts`).
pub fn gamma_eotf(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Decode a base64 data URI.
fn decode_data_uri(uri: &str) -> Result<Vec<u8>> {
    let comma = uri.find(',').context("invalid data URI")?;
    let encoded = &uri[comma + 1..];
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| anyhow::anyhow!("base64 decode failed: {e}"))
}

/// Read the raw bytes of a buffer source (BIN chunk or data URI).
fn read_buffer_source(buffer: &gltf::Buffer<'_>, blob: Option<&[u8]>) -> Result<Vec<u8>> {
    match buffer.source() {
        gltf::buffer::Source::Uri(uri) => {
            if uri.starts_with("data:") {
                decode_data_uri(uri)
            } else {
                bail!("external buffer URI not supported: {uri}")
            }
        }
        gltf::buffer::Source::Bin => {
            Ok(blob.context("GLB blob missing but buffer uses bin source")?.to_vec())
        }
    }
}

/// Read the raw bytes covered by a buffer view.
fn read_buffer_view_bytes(view: &gltf::buffer::View<'_>, blob: Option<&[u8]>) -> Result<Vec<u8>> {
    let buf_bytes = read_buffer_source(&view.buffer(), blob)?;
    let start = view.offset();
    let len = view.length();
    if start + len > buf_bytes.len() {
        bail!("buffer view out of bounds");
    }
    Ok(buf_bytes[start..start + len].to_vec())
}

/// Read the raw bytes for a single accessor (no type conversion).
fn read_accessor_bytes(accessor: &gltf::Accessor<'_>, blob: Option<&[u8]>) -> Result<Vec<u8>> {
    let view = accessor.view().context("accessor missing buffer view")?;
    let buf_bytes = read_buffer_source(&view.buffer(), blob)?;
    let start = view.offset() + accessor.offset();
    let len = view.length();
    if start + len > buf_bytes.len() {
        bail!("accessor out of bounds");
    }
    Ok(buf_bytes[start..start + len].to_vec())
}

/// Read an accessor as `Vec<f32>`, handling normalization and integer types.
fn read_accessor_f32(accessor: &gltf::Accessor<'_>, blob: Option<&[u8]>) -> Result<Vec<f32>> {
    let bytes = read_accessor_bytes(accessor, blob)?;
    let normalized = accessor.normalized();
    let count = accessor.count();
    let multiplicity = accessor.dimensions().multiplicity();
    let total_elements = count * multiplicity;
    let mut out = Vec::with_capacity(total_elements);
    match accessor.data_type() {
        gltf::accessor::DataType::F32 => {
            for chunk in bytes.chunks_exact(4) {
                out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        gltf::accessor::DataType::U8 => {
            for &b in &bytes {
                out.push(if normalized { b as f32 / 255.0 } else { b as f32 });
            }
        }
        gltf::accessor::DataType::I8 => {
            for &b in &bytes {
                let v = b as i8 as f32;
                out.push(if normalized { (v / 127.0).clamp(-1.0, 1.0) } else { v });
            }
        }
        gltf::accessor::DataType::U16 => {
            for chunk in bytes.chunks_exact(2) {
                let v = u16::from_le_bytes([chunk[0], chunk[1]]);
                out.push(if normalized { v as f32 / 65535.0 } else { v as f32 });
            }
        }
        gltf::accessor::DataType::I16 => {
            for chunk in bytes.chunks_exact(2) {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]) as f32;
                out.push(if normalized { (v / 32767.0).clamp(-1.0, 1.0) } else { v });
            }
        }
        gltf::accessor::DataType::U32 => {
            for chunk in bytes.chunks_exact(4) {
                let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                out.push(v as f32);
            }
        }
    }
    Ok(out)
}

/// Read an accessor as `Vec<u32>` (for indices).
fn read_accessor_u32(accessor: &gltf::Accessor<'_>, blob: Option<&[u8]>) -> Result<Vec<u32>> {
    let bytes = read_accessor_bytes(accessor, blob)?;
    let mut out = Vec::with_capacity(accessor.count());
    match accessor.data_type() {
        gltf::accessor::DataType::U8 => {
            for &b in &bytes {
                out.push(b as u32);
            }
        }
        gltf::accessor::DataType::U16 => {
            for chunk in bytes.chunks_exact(2) {
                out.push(u16::from_le_bytes([chunk[0], chunk[1]]) as u32);
            }
        }
        gltf::accessor::DataType::U32 => {
            for chunk in bytes.chunks_exact(4) {
                out.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        _ => bail!("unsupported data type for u32 accessor"),
    }
    Ok(out)
}

/// Read an accessor as `Vec<u16>` (for skin joints).
fn read_accessor_u16(accessor: &gltf::Accessor<'_>, blob: Option<&[u8]>) -> Result<Vec<u16>> {
    let bytes = read_accessor_bytes(accessor, blob)?;
    let mut out = Vec::with_capacity(accessor.count());
    match accessor.data_type() {
        gltf::accessor::DataType::U8 => {
            for &b in &bytes {
                out.push(b as u16);
            }
        }
        gltf::accessor::DataType::U16 => {
            for chunk in bytes.chunks_exact(2) {
                out.push(u16::from_le_bytes([chunk[0], chunk[1]]));
            }
        }
        _ => bail!("unsupported data type for u16 accessor"),
    }
    Ok(out)
}

/// Read an accessor as `Vec<u8>`.
fn read_accessor_u8(accessor: &gltf::Accessor<'_>, blob: Option<&[u8]>) -> Result<Vec<u8>> {
    let bytes = read_accessor_bytes(accessor, blob)?;
    Ok(bytes)
}

impl LoadedGltf {
    pub fn from_glb(bytes: &[u8]) -> Result<Self> {
        crate::log::debug("Parsing GLB binary...");
        let gltf = gltf::Gltf::from_slice(bytes).context("Failed to parse GLB")?;
        let doc = gltf.document;
        let blob = gltf.blob.as_deref();

        let root_extensions = doc
            .extensions()
            .cloned()
            .unwrap_or_default();
        let extensions_used: Vec<String> = doc
            .extensions_used()
            .map(|s| s.to_string())
            .collect();
        crate::log::debug(&format!(
            "GLB parsed: {} images, {} textures, {} materials, {} meshes, {} skins, {} nodes, {} animations",
            doc.images().len(),
            doc.textures().len(),
            doc.materials().len(),
            doc.meshes().len(),
            doc.skins().len(),
            doc.nodes().len(),
            doc.animations().len()
        ));
        crate::log::info(&format!(
            "glTF extensions used: {}",
            if extensions_used.is_empty() {
                "none".to_string()
            } else {
                extensions_used.join(", ")
            }
        ));

        let mut scene = Scene::default();

        // ---- images ----
        for image in doc.images() {
            let (rgba, width, height) = load_image(&image, blob)?;
            scene.images.push(ImageData {
                name: image.name().unwrap_or("").to_string(),
                width,
                height,
                rgba,
            });
        }

        // ---- textures ----
        for texture in doc.textures() {
            let sampler = texture.sampler();
            scene.textures.push(Texture {
                name: texture.name().unwrap_or("").to_string(),
                image: texture.source().index(),
                wrap_s: wrap_mode(sampler.wrap_s()),
                wrap_t: wrap_mode(sampler.wrap_t()),
                mag_filter: sampler.mag_filter().map(|f| f as u32),
                min_filter: sampler.min_filter().map(|f| f as u32),
            });
        }

        // ---- materials ----
        for material in doc.materials() {
            scene.materials.push(build_material(&material));
        }

        // ---- cameras ----
        for camera in doc.cameras() {
            let cam = match camera.projection() {
                gltf::camera::Projection::Perspective(p) => Some(PerspectiveCamera {
                    fovy: p.yfov(),
                    aspect: p.aspect_ratio().unwrap_or(16.0 / 9.0),
                    near: p.znear(),
                    far: p.zfar().unwrap_or(100.0),
                }),
                gltf::camera::Projection::Orthographic(_) => None,
            };
            scene.cameras.push(cam);
        }

        // ---- meshes ----
        for mesh in doc.meshes() {
            let mut primitives = Vec::new();
            for primitive in mesh.primitives() {
                primitives.push(build_primitive(&primitive, blob));
            }
            scene.meshes.push(Mesh {
                name: mesh.name().unwrap_or("").to_string(),
                primitives,
            });
        }

        // ---- skins ----
        for skin in doc.skins() {
            let mut inverse_bind_matrices = Vec::new();
            if let Some(ibm_accessor) = skin.inverse_bind_matrices() {
                let values = read_accessor_f32(&ibm_accessor, blob)?;
                for chunk in values.chunks_exact(16) {
                    inverse_bind_matrices.push(Mat4::from_cols_slice(chunk));
                }
            } else {
                for _ in skin.joints() {
                    inverse_bind_matrices.push(Mat4::IDENTITY);
                }
            }
            scene.skins.push(Skin {
                name: skin.name().unwrap_or("").to_string(),
                joints: skin.joints().map(|j| j.index()).collect(),
                inverse_bind_matrices,
                skeleton: skin.skeleton().map(|s| s.index()),
            });
        }

        // ---- nodes ----
        let mut nodes: Vec<Node> = Vec::new();
        for node in doc.nodes() {
            let mut n = Node::new(node.index());
            n.name = node.name().unwrap_or("").to_string();
            let (translation, rotation, scale) = match node.transform() {
                gltf::scene::Transform::Matrix { matrix } => {
                    let (s, r, t) = Mat4::from_cols_array_2d(&matrix).to_scale_rotation_translation();
                    (t, r, s)
                }
                gltf::scene::Transform::Decomposed {
                    translation,
                    rotation,
                    scale,
                } => {
                    let t = Vec3::from(translation);
                    let r = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
                    let s = Vec3::from(scale);
                    (t, r, s)
                }
            };
            n.translation = translation;
            n.rotation = rotation;
            n.scale = scale;
            if let Some(mesh) = node.mesh() {
                n.mesh = Some(mesh.index());
            }
            if let Some(skin) = node.skin() {
                n.skin = Some(skin.index());
            }
            if let Some(camera) = node.camera() {
                n.camera = Some(camera.index());
            }
            nodes.push(n);
        }

        // parent-child relationships
        for node in doc.nodes() {
            let parent_index = node.index();
            for child in node.children() {
                let child_index = child.index();
                nodes[parent_index].children.push(child_index);
                nodes[child_index].parent = Some(parent_index);
            }
        }

        // root nodes
        let root_nodes: Vec<usize> = doc
            .default_scene()
            .map(|s| s.nodes().map(|n| n.index()).collect())
            .or_else(|| doc.scenes().next().map(|s| s.nodes().map(|n| n.index()).collect()))
            .unwrap_or_else(|| {
                nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.parent.is_none())
                    .map(|(i, _)| i)
                    .collect()
            });

        scene.nodes = nodes;
        scene.root_nodes = root_nodes;

        // ---- animations ----
        for animation in doc.animations() {
            scene.animations.push(build_animation(&animation, blob));
        }

        Ok(LoadedGltf {
            scene,
            root_extensions,
            extensions_used,
        })
    }

    pub fn root_extension(&self, name: &str) -> Option<&serde_json::Value> {
        self.root_extensions.get(name)
    }

    pub fn extensions_used_contains(&self, name: &str) -> bool {
        self.extensions_used.iter().any(|e| e == name)
    }

    /// glTF animation channel lookup helper.
    pub fn find_animation(&self, name: &str) -> Option<usize> {
        self.scene
            .animations
            .iter()
            .position(|a| a.name == name || a.name.is_empty())
    }
}

fn wrap_mode(wrap: gltf::texture::WrappingMode) -> WrapMode {
    match wrap {
        gltf::texture::WrappingMode::ClampToEdge => WrapMode::ClampToEdge,
        gltf::texture::WrappingMode::MirroredRepeat => WrapMode::MirroredRepeat,
        gltf::texture::WrappingMode::Repeat => WrapMode::Repeat,
    }
}

fn load_image(image: &gltf::Image<'_>, blob: Option<&[u8]>) -> Result<(Vec<u8>, u32, u32)> {
    let bytes = match image.source() {
        gltf::image::Source::View { view, .. } => read_buffer_view_bytes(&view, blob)?,
        gltf::image::Source::Uri { uri, .. } => {
            if uri.starts_with("data:") {
                decode_data_uri(uri)?
            } else {
                let path = Path::new(uri);
                std::fs::read(path).context(format!("Failed to read external image: {uri}"))?
            }
        }
    };

    let decoded = image::load_from_memory(&bytes)
        .with_context(|| "Failed to decode texture image".to_string())?
        .to_rgba8();
    let (width, height) = (decoded.width(), decoded.height());
    Ok((decoded.into_raw(), width, height))
}

fn build_material(material: &gltf::Material<'_>) -> Material {
    let mut m = Material::default();
    m.name = material.name().unwrap_or("").to_string();

    let pbr = material.pbr_metallic_roughness();
    let base_color = pbr.base_color_factor();
    m.color = Vec4::new(base_color[0], base_color[1], base_color[2], base_color[3]);
    m.metallic = pbr.metallic_factor();
    m.roughness = pbr.roughness_factor();
    if let Some(tex) = pbr.base_color_texture() {
        m.base_color_texture = Some(tex.texture().index());
    }

    m.alpha_mode = match material.alpha_mode() {
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => AlphaMode::Mask,
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
    };
    m.alpha_cutoff = material.alpha_cutoff().unwrap_or(0.5);
    m.double_sided = material.double_sided();

    if let Some(tex) = material.normal_texture() {
        m.normal_map = Some(tex.texture().index());
        m.normal_scale = tex.scale();
    }
    if let Some(tex) = material.emissive_texture() {
        m.emissive_map = Some(tex.texture().index());
    }
    let emissive = material.emissive_factor();
    m.emissive = Vec3::new(emissive[0], emissive[1], emissive[2]);

    // MToon extensions
    if let Some(extensions) = material.extensions() {
        if let Some(mtoon) = extensions.get("VRMC_materials_mtoon") {
            parse_mtoon(&mut m, mtoon);
        }
    }

    m
}

fn parse_mtoon(material: &mut Material, json: &serde_json::Value) {
    let obj = match json.as_object() {
        Some(o) => o,
        None => return,
    };
    let num = |key: &str| -> Option<f32> {
        obj.get(key).and_then(|v| v.as_f64()).map(|f| f as f32)
    };
    let vec3 = |key: &str| -> Option<Vec3> {
        obj.get(key).and_then(|v| v.as_array()).and_then(|a| {
            if a.len() >= 3 {
                Some(Vec3::new(
                    a[0].as_f64()? as f32,
                    a[1].as_f64()? as f32,
                    a[2].as_f64()? as f32,
                ))
            } else {
                None
            }
        })
    };
    let tex = |key: &str| -> Option<usize> {
        obj.get(key)
            .and_then(|v| v.as_object())
            .and_then(|o| o.get("index"))
            .and_then(|v| v.as_u64())
            .map(|u| u as usize)
    };

    material.kind = MaterialKind::Mtoon;
    if let Some(v) = tex("shadeMultiplyTexture") {
        material.shade_multiply_texture = Some(v);
    }
    if let Some(v) = tex("shadingShiftTexture") {
        material.shading_shift_texture = Some(v);
    }
    material.shading_shift = num("shadingShiftFactor").unwrap_or(0.0);
    material.shading_shift_texture_scale = num("shadingShiftTextureScale").unwrap_or(1.0);
    material.shading_toony = num("shadingToonyFactor").unwrap_or(0.9);
    material.gi_equalization = num("giEqualizationFactor").unwrap_or(0.9);
    if let Some(v) = vec3("shadeColorFactor") {
        material.shade_color = v;
    }
    if let Some(v) = tex("matcapTexture") {
        material.matcap_texture = Some(v);
    }
    if let Some(v) = vec3("matcapFactor") {
        material.matcap_color = v;
    }
    if let Some(v) = tex("rimMultiplyTexture") {
        material.rim_multiply_texture = Some(v);
    }
    if let Some(v) = vec3("parametricRimColorFactor") {
        material.rim_color = v;
    }
    material.rim_lighting_mix = num("rimLightingMix").unwrap_or(0.0);
    material.rim_fresnel_power = num("rimFresnelPower").unwrap_or(1.0);
    material.rim_lift = num("rimLift").unwrap_or(0.0);
    if let Some(v) = num("outlineWidthFactor") {
        material.outline_width = v;
        material.outline_width_mode = OutlineWidthMode::WorldCoordinates;
    }
    if let Some(v) = tex("outlineWidthMultiplyTexture") {
        material.outline_width_multiply_texture = Some(v);
    }
    if let Some(v) = vec3("outlineColorFactor") {
        material.outline_color = v;
    }
    material.outline_lighting_mix = num("outlineLightingMix").unwrap_or(1.0);
    if let Some(v) = tex("uvAnimationMaskTexture") {
        material.uv_animation_mask = Some(v);
    }
    material.uv_scroll_x_speed = num("uvAnimationScrollXSpeedFactor").unwrap_or(0.0);
    material.uv_scroll_y_speed = num("uvAnimationScrollYSpeedFactor").unwrap_or(0.0);
    material.uv_rotation_speed = num("uvAnimationRotationSpeedFactor").unwrap_or(0.0);
}

fn build_primitive(primitive: &gltf::Primitive<'_>, blob: Option<&[u8]>) -> Primitive {
    let mut p = Primitive {
        positions: Vec::new(),
        normals: None,
        texcoords: None,
        colors: None,
        joints: None,
        weights: None,
        tangents: None,
        indices: None,
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
        material: primitive.material().index(),
        mode: primitive.mode() as u32,
    };

    if let Some(index_accessor) = primitive.indices() {
        if let Ok(data) = read_accessor_u32(&index_accessor, blob) {
            p.indices = Some(data);
        }
    }

    for (semantic, accessor) in primitive.attributes() {
        match semantic {
            gltf::mesh::Semantic::Positions => {
                if let Ok(data) = read_accessor_f32(&accessor, blob) {
                    p.positions = data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                }
            }
            gltf::mesh::Semantic::Normals => {
                if let Ok(data) = read_accessor_f32(&accessor, blob) {
                    p.normals = Some(data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect());
                }
            }
            gltf::mesh::Semantic::Tangents => {
                if let Ok(data) = read_accessor_f32(&accessor, blob) {
                    p.tangents = Some(data.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
                }
            }
            gltf::mesh::Semantic::Colors(set) => {
                if set == 0 {
                    if let Ok(data) = read_accessor_f32(&accessor, blob) {
                        let dims = accessor.dimensions();
                        if dims == gltf::accessor::Dimensions::Vec4 {
                            p.colors = Some(data.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
                        } else if dims == gltf::accessor::Dimensions::Vec3 {
                            p.colors = Some(data.chunks_exact(3).map(|c| [c[0], c[1], c[2], 1.0]).collect());
                        }
                    }
                }
            }
            gltf::mesh::Semantic::TexCoords(set) => {
                if set == 0 {
                    if let Ok(data) = read_accessor_f32(&accessor, blob) {
                        p.texcoords = Some(data.chunks_exact(2).map(|c| [c[0], c[1]]).collect());
                    }
                }
            }
            gltf::mesh::Semantic::Joints(set) => {
                if set == 0 {
                    let dtype = accessor.data_type();
                    let joints = match dtype {
                        gltf::accessor::DataType::U8 => {
                            read_accessor_u8(&accessor, blob)
                                .ok()
                                .map(|v| v.chunks_exact(4).map(|c| [c[0] as u16, c[1] as u16, c[2] as u16, c[3] as u16]).collect())
                        }
                        gltf::accessor::DataType::U16 => {
                            read_accessor_u16(&accessor, blob)
                                .ok()
                                .map(|v| v.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect())
                        }
                        _ => None,
                    };
                    p.joints = joints;
                }
            }
            gltf::mesh::Semantic::Weights(set) => {
                if set == 0 {
                    if let Ok(data) = read_accessor_f32(&accessor, blob) {
                        p.weights = Some(data.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
                    }
                }
            }
        }
    }

    for target in primitive.morph_targets() {
        let mut mt = MorphTarget {
            positions: None,
            normals: None,
        };
        if let Some(accessor) = target.positions() {
            if let Ok(data) = read_accessor_f32(&accessor, blob) {
                mt.positions = Some(data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect());
            }
        }
        if let Some(accessor) = target.normals() {
            if let Ok(data) = read_accessor_f32(&accessor, blob) {
                mt.normals = Some(data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect());
            }
        }
        p.morph_targets.push(mt);
    }

    p.morph_weights = vec![0.0; p.morph_targets.len()];
    p
}

fn build_animation(animation: &gltf::Animation<'_>, blob: Option<&[u8]>) -> GltfAnimation {
    let mut samplers = Vec::new();
    for sampler in animation.samplers() {
        let input = read_accessor_f32(&sampler.input(), blob).unwrap_or_default();
        let output = read_accessor_f32(&sampler.output(), blob).unwrap_or_default();
        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Linear => "LINEAR".to_string(),
            gltf::animation::Interpolation::Step => "STEP".to_string(),
            gltf::animation::Interpolation::CubicSpline => "CUBICSPLINE".to_string(),
        };
        let component_count = output.len() / input.len().max(1);
        samplers.push(GltfAnimationSampler {
            input,
            output,
            interpolation,
            component_count,
        });
    }

    let mut channels = Vec::new();
    for channel in animation.channels() {
        channels.push(GltfAnimationChannel {
            sampler: channel.sampler().index(),
            node: channel.target().node().index(),
            path: match channel.target().property() {
                gltf::animation::Property::Translation => "translation".to_string(),
                gltf::animation::Property::Rotation => "rotation".to_string(),
                gltf::animation::Property::Scale => "scale".to_string(),
                gltf::animation::Property::MorphTargetWeights => "weights".to_string(),
            },
        });
    }

    let duration = samplers
        .iter()
        .filter_map(|s| {
            s.input
                .iter()
                .copied()
                .fold(None, |acc, v| Some(acc.map_or(v, |a: f32| a.max(v))))
        })
        .fold(0.0f32, |a, b| a.max(b));

    GltfAnimation {
        name: animation.name().unwrap_or("").to_string(),
        samplers,
        channels,
        duration,
    }
}

/// Apply VRM 0.x `materialProperties` (ported from `VRMMaterialsV0CompatPlugin`).
pub fn apply_v0_material_properties(scene: &mut Scene, v0_materials: &[serde_json::Value]) {
    for (material_index, material_properties) in v0_materials.iter().enumerate() {
        let Some(material) = scene.materials.get_mut(material_index) else {
            continue;
        };
        let shader = material_properties
            .get("shader")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let vector = |name: &str| -> Option<Vec4> {
            material_properties
                .get("vectorProperties")
                .and_then(|vp| vp.get(name))
                .and_then(|v| {
                    let a = v.as_array()?;
                    Some(Vec4::new(
                        a[0].as_f64()? as f32,
                        a[1].as_f64()? as f32,
                        a[2].as_f64()? as f32,
                        a[3].as_f64()? as f32,
                    ))
                })
        };
        let float = |name: &str| -> Option<f32> {
            material_properties
                .get("floatProperties")
                .and_then(|fp| fp.get(name))
                .and_then(|v| v.as_f64())
                .map(|f| f as f32)
        };
        let texture = |name: &str| -> Option<usize> {
            material_properties
                .get("textureProperties")
                .and_then(|tp| tp.get(name))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
        };
        let keyword = |name: &str| -> bool {
            material_properties
                .get("keywordMap")
                .and_then(|km| km.get(name))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };

        if shader == "VRM/MToon" {
            material.kind = MaterialKind::Mtoon;
            if let Some(v) = vector("_Color") {
                material.color = v;
            }
            if let Some(v) = texture("_MainTex") {
                material.base_color_texture = Some(v);
            }
            if let Some(v) = vector("_ShadeColor") {
                material.shade_color = v.truncate();
            }
            if let Some(v) = texture("_ShadeTexture") {
                material.shade_multiply_texture = Some(v);
            }
            if let Some(v) = float("_ShadingToony") {
                material.shading_toony = v;
            }
            if let Some(v) = float("_ShadingShift") {
                material.shading_shift = v;
            }
            if let Some(v) = float("_GiEqualization") {
                material.gi_equalization = v;
            }
            if let Some(v) = vector("_EmissionColor") {
                material.emissive = v.truncate() * v.w;
            }
            if let Some(v) = texture("_EmissionMap") {
                material.emissive_map = Some(v);
            }
            if let Some(v) = float("_OutlineWidth") {
                material.outline_width = v;
                material.outline_width_mode = OutlineWidthMode::WorldCoordinates;
            }
            if let Some(v) = texture("_OutlineWidthTexture") {
                material.outline_width_multiply_texture = Some(v);
            }
            if let Some(v) = vector("_OutlineColor") {
                material.outline_color = v.truncate();
            }
            if let Some(v) = float("_OutlineLightingMix") {
                material.outline_lighting_mix = v;
            }
            if let Some(v) = float("_RimLightingMix") {
                material.rim_lighting_mix = v;
            }
            if let Some(v) = float("_RimFresnelPower") {
                material.rim_fresnel_power = v;
            }
            if let Some(v) = float("_RimLift") {
                material.rim_lift = v;
            }
            if let Some(v) = vector("_RimColor") {
                material.rim_color = v.truncate();
            }
            if let Some(v) = texture("_RimTexture") {
                material.rim_multiply_texture = Some(v);
            }
            if let Some(v) = vector("_MatCapColor") {
                material.matcap_color = v.truncate();
            }
            if let Some(v) = texture("_MatCap") {
                material.matcap_texture = Some(v);
            }
            material.normal_map = texture("_BumpMap");
            material.unlit = keyword("_MToonCullMode");
            material.v0_compat_shade = true;
        }

        if keyword("_ALPHATEST_ON") {
            material.alpha_mode = AlphaMode::Mask;
            material.alpha_cutoff = float("_Cutoff").unwrap_or(0.5);
        } else if keyword("_ALPHABLEND_ON") || keyword("_ALPHAPREMULTIPLY_ON") {
            material.alpha_mode = AlphaMode::Blend;
        }
        if keyword("_NORMALMAP") {
            material.normal_map = texture("_BumpMap");
            material.normal_scale = float("_BumpScale").unwrap_or(1.0);
        }
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn log_warn(msg: &str) {
    eprintln!("[WARN] {msg}");
}
