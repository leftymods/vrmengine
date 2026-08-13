//! VRM loader, ported from `@pixiv/three-vrm` loader plugins.
//!
//! Parses the `VRMC_vrm` (VRM 1.0) and `VRM` (VRM 0.x) extensions out of a loaded glTF and
//! constructs the `VRM` struct (humanoid, expressions, look-at, first-person, spring bones, meta).

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use glam::{Vec2, Vec3};
use serde_json::Value;

use crate::gltf_loader::{log_warn, LoadedGltf};
use crate::vrm::expression::{
    v0_material_color_kind, v0_preset_is_binary, v0_preset_to_v1, Bind, Expression, ExpressionManager,
    MaterialColorBind, MaterialColorKind, MorphTargetBind, OverrideType, PresetName, TextureTransformBind,
};
use crate::vrm::firstperson::{FirstPerson, FirstPersonFlag, MeshAnnotation};
use crate::vrm::humanoid::{HumanBoneName, HumanBones, Humanoid};
use crate::vrm::lookat::{Applier, BoneApplier, ExpressionApplier, LookAt, RangeMap};
use crate::vrm::meta::{
    V0AllowedUserName, V0CommercialUsagePermission, V0LicenseName, V0UsagePermission, Vrm0Meta, Vrm1Meta,
    VrmMeta,
};
use crate::vrm::model::VRM;
use crate::vrm::springbone::{
    Collider, ColliderGroup, ColliderShape, Joint, JointSettings, SpringBoneManager,
};

// ----------------------------- JSON helpers -----------------------------

fn jobj(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    v.as_object()
}

fn jarr<'a>(v: &'a Value) -> Option<&'a [Value]> {
    v.as_array().map(|a| a.as_slice())
}

fn jstr(v: &Value) -> Option<&str> {
    v.as_str()
}

fn jnum(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn ju32(v: &Value) -> Option<u32> {
    v.as_u64().map(|u| u as u32)
}

fn jbool(v: &Value) -> Option<bool> {
    v.as_bool()
}

fn get<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    obj.get(key)
}

fn f32_of(v: &Value) -> Option<f32> {
    jnum(v).map(|f| f as f32)
}

fn vec2_of(v: &Value) -> Option<Vec2> {
    let a = jarr(v)?;
    Some(Vec2::new(f32_of(&a[0])?, f32_of(&a[1])?))
}

fn vec3_of(v: &Value) -> Option<Vec3> {
    let a = jarr(v)?;
    Some(Vec3::new(f32_of(&a[0])?, f32_of(&a[1])?, f32_of(&a[2])?))
}

fn f32_array(v: &Value) -> Vec<f32> {
    jarr(v)
        .map(|a| a.iter().filter_map(f32_of).collect())
        .unwrap_or_default()
}

// ----------------------------- Entry point -----------------------------

/// Load a VRM model from a GLB byte slice.
pub fn load_vrm(bytes: &[u8]) -> Result<VRM> {
    crate::log::info("VRM loader: parsing GLB...");
    let mut loaded = LoadedGltf::from_glb(bytes).context("Failed to parse GLB")?;

    // The component parsers need `&mut Scene` (e.g. for the normalized rig); move the scene out
    // of `loaded` temporarily and put it back before we read the extension JSON.
    let mut scene = std::mem::take(&mut loaded.scene);
    let mut humanoid = None;
    let mut expressions = None;
    let mut look_at = None;
    let mut first_person = None;
    let mut spring_bone_manager = None;
    let mut meta = None;

    let is_v1 = loaded.extensions_used_contains("VRMC_vrm");
    let is_v0 = loaded.extensions_used_contains("VRM");

    if is_v1 {
        crate::log::info("VRM loader: detected VRM 1.0 (VRMC_vrm)");
        import_vrm1(&loaded, &mut scene, &mut meta, &mut humanoid, &mut expressions, &mut look_at, &mut first_person)?;
    } else if is_v0 {
        crate::log::info("VRM loader: detected VRM 0.x (VRM)");
        import_vrm0(&loaded, &mut scene, &mut meta, &mut humanoid, &mut expressions, &mut look_at, &mut first_person, &mut spring_bone_manager)?;
    } else {
        bail!("VRM loader: neither VRMC_vrm nor VRM extension is present");
    }

    // Spring bones live in the separate `VRMC_springBone` extension (v1).
    if loaded.extensions_used_contains("VRMC_springBone") {
        if let Some(sb) = loaded.root_extension("VRMC_springBone") {
            spring_bone_manager = import_vrm1_spring_bone(sb, &scene);
            if let Some(sbm) = &spring_bone_manager {
                crate::log::debug(&format!(
                    "VRM loader: {} spring joints, {} colliders",
                    sbm.joints.len(),
                    sbm.colliders.len()
                ));
            }
        }
    }

    if let Some(m) = &meta {
        crate::log::info(&format!(
            "VRM meta: \"{}\" (type {})",
            m.title().unwrap_or("unknown"),
            m.meta_type()
        ));
    }
    if let Some(h) = &humanoid {
        crate::log::debug(&format!(
            "Humanoid: {} bones assigned, auto_update={}",
            h.raw_rig.human_bones.len(),
            h.auto_update_human_bones
        ));
    }
    if let Some(e) = &expressions {
        crate::log::debug(&format!("{} expressions registered", e.expressions.len()));
    }
    if let Some(l) = &look_at {
        crate::log::debug(&format!(
            "LookAt: type={}",
            match l.applier {
                crate::vrm::lookat::Applier::Bone(_) => "bone",
                crate::vrm::lookat::Applier::Expression(_) => "expression",
            }
        ));
    }
    if let Some(fp) = &first_person {
        crate::log::debug(&format!(
            "FirstPerson: {} mesh annotations",
            fp.mesh_annotations.len()
        ));
    }
    crate::log::info("VRM model fully loaded");

    Ok(VRM {
        scene,
        humanoid,
        expressions,
        look_at,
        first_person,
        spring_bone_manager,
        meta,
    })
}

// ----------------------------- VRM 1.0 -----------------------------

#[allow(clippy::too_many_arguments)]
fn import_vrm1(
    loaded: &LoadedGltf,
    scene: &mut crate::scene::Scene,
    meta: &mut Option<VrmMeta>,
    humanoid: &mut Option<Humanoid>,
    expressions: &mut Option<ExpressionManager>,
    look_at: &mut Option<LookAt>,
    first_person: &mut Option<FirstPerson>,
) -> Result<()> {
    let ext = loaded
        .root_extension("VRMC_vrm")
        .and_then(jobj)
        .context("VRMC_vrm extension missing")?;

    // spec version check
    if let Some(spec) = get(ext, "specVersion").and_then(jstr) {
        if spec != "1.0" && spec != "1.0-beta" {
            log_warn(&format!("VRM loader: unknown VRMC_vrm specVersion \"{spec}\""));
        }
    }

    // meta
    if let Some(m) = get(ext, "meta") {
        *meta = Some(VrmMeta::Vrm1(parse_vrm1_meta(m)));
    }

    // humanoid
    if let Some(h) = get(ext, "humanoid") {
        *humanoid = Some(parse_vrm1_humanoid(h, scene)?);
    }

    // expressions
    if let Some(e) = get(ext, "expressions") {
        *expressions = Some(parse_vrm1_expressions(e, scene));
    } else {
        *expressions = Some(ExpressionManager::new());
    }

    // look-at (requires humanoid + expressions)
    if let Some(la) = get(ext, "lookAt") {
        if let Some(h) = humanoid.as_ref() {
            *look_at = parse_vrm1_look_at(la, h, scene);
        }
    }

    // first-person
    if let Some(fp) = get(ext, "firstPerson") {
        let head = humanoid.as_ref().and_then(|h| h.get_raw_bone_node(HumanBoneName::Head));
        *first_person = Some(parse_vrm1_first_person(fp, scene, head));
    }

    Ok(())
}

fn parse_vrm1_meta(v: &Value) -> Vrm1Meta {
    let Some(o) = jobj(v) else {
        return Vrm1Meta::default();
    };

    Vrm1Meta {
        name: get(o, "name").and_then(jstr).map(String::from),
        version: get(o, "version").and_then(jstr).map(String::from),
        authors: get(o, "authors")
            .and_then(jarr)
            .map(|a| a.iter().filter_map(jstr).map(String::from).collect())
            .unwrap_or_default(),
        copyright_information: get(o, "copyrightInformation").and_then(jstr).map(String::from),
        contact_information: get(o, "contactInformation").and_then(jstr).map(String::from),
        references: get(o, "references")
            .and_then(jarr)
            .map(|a| a.iter().filter_map(jstr).map(String::from).collect())
            .unwrap_or_default(),
        third_party_licenses: get(o, "thirdPartyLicenses").and_then(jstr).map(String::from),
        thumbnail_image: get(o, "thumbnailImage").and_then(ju32).map(|i| i as usize),
        license_url: get(o, "licenseUrl").and_then(jstr).map(String::from),
        avatar_permission: get(o, "avatarPermission").and_then(jstr).and_then(crate::vrm::meta::V1AvatarPermission::from_str),
        allow_excessively_violent_usage: get(o, "allowExcessivelyViolentUsage").and_then(jbool).unwrap_or(false),
        allow_excessively_sexual_usage: get(o, "allowExcessivelySexualUsage").and_then(jbool).unwrap_or(false),
        violent_usage_description: get(o, "violentUsageDescription").and_then(jstr).map(String::from),
        sexual_usage_description: get(o, "sexualUsageDescription").and_then(jstr).map(String::from),
        commercial_usage: get(o, "commercialUsage")
            .and_then(jstr)
            .and_then(crate::vrm::meta::V1CommercialUsage::from_str),
        credit_notation: get(o, "creditNotation")
            .and_then(jstr)
            .and_then(crate::vrm::meta::V1CreditNotation::from_str),
        allow_redistribution: get(o, "allowRedistribution").and_then(jbool).unwrap_or(false),
        modification: get(o, "modification")
            .and_then(jstr)
            .and_then(crate::vrm::meta::V1Modification::from_str),
        other_license_url: get(o, "otherLicenseUrl").and_then(jstr).map(String::from),
        other_permission_url: get(o, "otherPermissionUrl").and_then(jstr).map(String::from),
    }
}

fn parse_vrm1_humanoid(v: &Value, scene: &mut crate::scene::Scene) -> Result<Humanoid> {
    let o = jobj(v).context("humanoid must be an object")?;
    let mut bones: HumanBones = HashMap::new();

    if let Some(human_bones) = get(o, "humanBones").and_then(jobj) {
        for (name, value) in human_bones {
            let Some(bone_name) = HumanBoneName::from_str(name) else {
                continue;
            };
            let Some(node) = value.as_object().and_then(|bo| get(bo, "node")).and_then(ju32) else {
                continue;
            };
            let node = node as usize;
            if scene.nodes.get(node).is_some() {
                bones.insert(bone_name, node);
            }
        }
    }

    // required bones
    for required in HumanBoneName::REQUIRED {
        if !bones.contains_key(required) {
            bail!("VRM loader: required human bone {:?} is missing", required);
        }
    }

    let auto_update = get(o, "autoUpdate").and_then(jbool).unwrap_or(true);
    Ok(Humanoid::new(scene, bones, auto_update))
}

fn parse_vrm1_expressions(v: &Value, scene: &crate::scene::Scene) -> ExpressionManager {
    let mut manager = ExpressionManager::new();
    let Some(expressions) = jarr(v) else {
        return manager;
    };

    for (i, expr_json) in expressions.iter().enumerate() {
        let Some(o) = jobj(expr_json) else {
            continue;
        };

        let preset = get(o, "presetName").and_then(jstr).and_then(PresetName::from_str);
        let name = get(o, "name")
            .and_then(jstr)
            .map(String::from)
            .or_else(|| preset.map(|p| p.as_str().to_string()))
            .unwrap_or_else(|| format!("expression_{i}"));

        let is_binary = get(o, "isBinary").and_then(jbool).unwrap_or(false);
        let override_blink = OverrideType::from_str(get(o, "overrideBlink").and_then(jstr).unwrap_or("none"));
        let override_look_at = OverrideType::from_str(get(o, "overrideLookAt").and_then(jstr).unwrap_or("none"));
        let override_mouth = OverrideType::from_str(get(o, "overrideMouth").and_then(jstr).unwrap_or("none"));

        let mut binds = Vec::new();

        // morph target binds
        for bind in get(o, "morphTargetBinds").and_then(jarr).unwrap_or(&[]) {
            if let Some(m) = parse_vrm1_morph_target_bind(bind, scene) {
                binds.push(Bind::Morph(m));
            }
        }

        // material color binds
        for bind in get(o, "materialColorBinds").and_then(jarr).unwrap_or(&[]) {
            if let Some(m) = parse_vrm1_material_color_bind(bind, scene) {
                binds.push(Bind::Color(m));
            }
        }

        // texture transform binds
        for bind in get(o, "textureTransformBinds").and_then(jarr).unwrap_or(&[]) {
            if let Some(m) = parse_vrm1_texture_transform_bind(bind, scene) {
                binds.push(Bind::TextureTransform(m));
            }
        }

        manager.register_expression(Expression {
            name,
            preset_name: preset,
            weight: 0.0,
            is_binary,
            override_blink,
            override_look_at,
            override_mouth,
            binds,
        });
    }

    manager
}

fn parse_vrm1_morph_target_bind(v: &Value, scene: &crate::scene::Scene) -> Option<MorphTargetBind> {
    let o = jobj(v)?;
    let index = get(o, "index").and_then(ju32)? as usize;
    let weight = get(o, "weight").and_then(f32_of).unwrap_or(1.0);

    let mesh = if let Some(node) = get(o, "node").and_then(ju32) {
        let node = node as usize;
        scene.node(node).mesh?
    } else if let Some(mesh_index) = get(o, "mesh").and_then(ju32) {
        mesh_index as usize
    } else {
        return None;
    };

    if scene.meshes.get(mesh).is_none() {
        return None;
    }

    Some(MorphTargetBind {
        mesh,
        target: index,
        weight,
    })
}

fn parse_vrm1_material_color_bind(v: &Value, scene: &crate::scene::Scene) -> Option<MaterialColorBind> {
    let o = jobj(v)?;
    let material = get(o, "material").and_then(ju32)? as usize;
    let kind = MaterialColorKind::from_str(get(o, "type").and_then(jstr)?)?;
    let target_value = f32_array(get(o, "targetValue")?);

    let mat = scene.materials.get(material)?;
    let (initial_rgb, initial_alpha) = current_material_color(mat, kind);
    let target_rgb = Vec3::new(target_value[0], target_value[1], target_value[2]);
    let target_alpha = if matches!(kind, MaterialColorKind::Color) {
        target_value.get(3).copied().unwrap_or(1.0)
    } else {
        1.0
    };

    let delta_rgb = target_rgb - initial_rgb;
    let delta_alpha = if matches!(kind, MaterialColorKind::Color) {
        Some(target_alpha - initial_alpha.unwrap_or(1.0))
    } else {
        None
    };

    Some(MaterialColorBind {
        material,
        kind,
        initial_rgb,
        delta_rgb,
        initial_alpha: if matches!(kind, MaterialColorKind::Color) {
            initial_alpha
        } else {
            None
        },
        delta_alpha,
    })
}

fn parse_vrm1_texture_transform_bind(v: &Value, scene: &crate::scene::Scene) -> Option<TextureTransformBind> {
    let o = jobj(v)?;
    let material = get(o, "material").and_then(ju32)? as usize;
    let scale = get(o, "scale").and_then(vec2_of).unwrap_or(Vec2::ONE);
    let offset = get(o, "offset").and_then(vec2_of).unwrap_or(Vec2::ZERO);

    if scene.materials.get(material).is_none() {
        return None;
    }

    Some(TextureTransformBind::new(material, scale, offset, scene))
}

fn parse_vrm1_look_at(v: &Value, humanoid: &Humanoid, scene: &crate::scene::Scene) -> Option<LookAt> {
    let o = jobj(v)?;
    let look_at_type = get(o, "type").and_then(jstr).unwrap_or("bone");
    let default_output_scale = if look_at_type == "expression" { 1.0 } else { 10.0 };

    let map_hi = import_vrm1_range_map(get(o, "rangeMapHorizontalInner"), default_output_scale);
    let map_ho = import_vrm1_range_map(get(o, "rangeMapHorizontalOuter"), default_output_scale);
    let map_vd = import_vrm1_range_map(get(o, "rangeMapVerticalDown"), default_output_scale);
    let map_vu = import_vrm1_range_map(get(o, "rangeMapVerticalUp"), default_output_scale);

    let applier = match look_at_type {
        "expression" => Applier::Expression(ExpressionApplier::new(map_hi, map_ho, map_vd, map_vu)),
        _ => Applier::Bone(BoneApplier::new(humanoid, scene, map_hi, map_ho, map_vd, map_vu)),
    };

    let mut look_at = LookAt::new(humanoid, applier);
    if let Some(offset) = get(o, "offsetFromHeadBone").and_then(vec3_of) {
        look_at.offset_from_head_bone = offset;
    } else {
        look_at.offset_from_head_bone = Vec3::new(0.0, 0.06, 0.0);
    }

    Some(look_at)
}

fn import_vrm1_range_map(v: Option<&Value>, default_output_scale: f32) -> RangeMap {
    let o = v.and_then(jobj);
    let input_max_value = o
        .and_then(|o| get(o, "inputMaxValue"))
        .and_then(f32_of)
        .unwrap_or(90.0)
        .max(0.01);
    let output_scale = o
        .and_then(|o| get(o, "outputScale"))
        .and_then(f32_of)
        .unwrap_or(default_output_scale);
    RangeMap::new(input_max_value, output_scale)
}

fn parse_vrm1_first_person(v: &Value, scene: &crate::scene::Scene, head: Option<usize>) -> FirstPerson {
    let o = jobj(v);
    let mut annotations = Vec::new();

    for node in &scene.nodes {
        if node.mesh.is_none() {
            continue;
        }
        let flag = o
            .and_then(|o| get(o, "meshAnnotations"))
            .and_then(jarr)
            .and_then(|arr| {
                arr.iter().find_map(|a| {
                    let ao = jobj(a)?;
                    let a_node = get(ao, "node").and_then(ju32)? as usize;
                    if a_node == node.index {
                        Some(FirstPersonFlag::from_v1(get(ao, "type").and_then(jstr).unwrap_or("auto")))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(FirstPersonFlag::Auto);

        annotations.push(MeshAnnotation {
            node: node.index,
            flag,
        });
    }

    FirstPerson::new(head, annotations)
}

// ----------------------------- VRM 0.x -----------------------------

#[allow(clippy::too_many_arguments)]
fn import_vrm0(
    loaded: &LoadedGltf,
    scene: &mut crate::scene::Scene,
    meta: &mut Option<VrmMeta>,
    humanoid: &mut Option<Humanoid>,
    expressions: &mut Option<ExpressionManager>,
    look_at: &mut Option<LookAt>,
    first_person: &mut Option<FirstPerson>,
    spring_bone_manager: &mut Option<SpringBoneManager>,
) -> Result<()> {
    let ext = loaded
        .root_extension("VRM")
        .and_then(jobj)
        .context("VRM extension missing")?;

    // meta
    if let Some(m) = get(ext, "meta") {
        *meta = Some(VrmMeta::Vrm0(parse_vrm0_meta(m)));
    }

    // humanoid
    if let Some(h) = get(ext, "humanoid") {
        *humanoid = Some(parse_vrm0_humanoid(h, scene)?);
    }

    // expressions (blendShapeMaster)
    if let Some(blend_shape_master) = get(ext, "blendShapeMaster") {
        *expressions = Some(parse_vrm0_expressions(blend_shape_master, scene));
    } else {
        *expressions = Some(ExpressionManager::new());
    }

    // first-person (provides lookAt range maps in v0)
    if let Some(first_person_json) = get(ext, "firstPerson") {
        let head = humanoid.as_ref().and_then(|h| h.get_raw_bone_node(HumanBoneName::Head));
        *first_person = Some(parse_vrm0_first_person(first_person_json, scene, head));

        // look-at
        if let Some(h) = humanoid.as_ref() {
            *look_at = parse_vrm0_look_at(first_person_json, h, scene);
        }
    }

    // spring bones (secondaryAnimation)
    if let Some(secondary) = get(ext, "secondaryAnimation") {
        *spring_bone_manager = import_vrm0_spring_bone(secondary, scene);
    }

    Ok(())
}

fn parse_vrm0_meta(v: &Value) -> Vrm0Meta {
    let Some(o) = jobj(v) else {
        return Vrm0Meta::default();
    };

    Vrm0Meta {
        title: get(o, "title").and_then(jstr).map(String::from),
        version: get(o, "version").and_then(jstr).map(String::from),
        author: get(o, "author").and_then(jstr).map(String::from),
        contact_information: get(o, "contactInformation").and_then(jstr).map(String::from),
        reference: get(o, "reference").and_then(jstr).map(String::from),
        texture: get(o, "texture").and_then(ju32).map(|i| i as usize),
        allowed_user_name: get(o, "allowedUserName").and_then(jstr).and_then(V0AllowedUserName::from_str),
        violent_usage_name: get(o, "violentUssageName").and_then(jstr).and_then(V0UsagePermission::from_str),
        sexual_usage_name: get(o, "sexualUssageName").and_then(jstr).and_then(V0UsagePermission::from_str),
        commercial_usage_name: get(o, "commercialUssageName")
            .and_then(jstr)
            .and_then(V0CommercialUsagePermission::from_str),
        other_permission_url: get(o, "otherPermissionUrl").and_then(jstr).map(String::from),
        license_name: get(o, "licenseName").and_then(jstr).and_then(V0LicenseName::from_str),
        other_license_url: get(o, "otherLicenseUrl").and_then(jstr).map(String::from),
    }
}

fn parse_vrm0_humanoid(v: &Value, scene: &mut crate::scene::Scene) -> Result<Humanoid> {
    let o = jobj(v).context("humanoid must be an object")?;
    let mut bones: HumanBones = HashMap::new();

    if let Some(human_bones) = get(o, "humanBones").and_then(jobj) {
        for (name, value) in human_bones {
            let Some(bone_name) = HumanBoneName::from_str(name) else {
                continue;
            };
            let Some(node) = value.as_object().and_then(|bo| get(bo, "node")).and_then(ju32) else {
                continue;
            };
            let node = node as usize;
            if scene.nodes.get(node).is_some() {
                bones.insert(bone_name, node);
            }
        }
    }

    for required in HumanBoneName::REQUIRED {
        if !bones.contains_key(required) {
            bail!("VRM loader: required human bone {:?} is missing", required);
        }
    }

    Ok(Humanoid::new(scene, bones, true))
}

fn parse_vrm0_expressions(v: &Value, scene: &crate::scene::Scene) -> ExpressionManager {
    let mut manager = ExpressionManager::new();
    let Some(o) = jobj(v) else {
        return manager;
    };

    let groups = get(o, "blendShapeGroups").and_then(jarr).unwrap_or(&[]);
    for (i, group) in groups.iter().enumerate() {
        let Some(g) = jobj(group) else {
            continue;
        };

        let preset_str = get(g, "presetName").and_then(jstr);
        let preset_name = match preset_str {
            Some("unknown") | None => None,
            Some("neutral") => Some(PresetName::Neutral),
            Some(other) => v0_preset_to_v1(other),
        };
        let is_binary = preset_str.map(|p| v0_preset_is_binary(p)).unwrap_or(false);

        let display_name = get(g, "name")
            .and_then(jstr)
            .map(String::from)
            .or_else(|| preset_name.map(|p| p.as_str().to_string()))
            .unwrap_or_else(|| format!("blendShape_{i}"));

        let mut binds = Vec::new();

        // morph target binds
        for bind in get(g, "binds").and_then(jarr).unwrap_or(&[]) {
            if let Some(m) = parse_vrm0_morph_target_bind(bind, scene) {
                binds.push(Bind::Morph(m));
            }
        }

        // material value binds (color + texture transform)
        for mv in get(g, "materialValues").and_then(jarr).unwrap_or(&[]) {
            if let Some(mv_obj) = jobj(mv) {
                let material_name = get(mv_obj, "materialName").and_then(jstr);
                let property = get(mv_obj, "propertyName").and_then(jstr).unwrap_or("");
                let target_value = f32_array(get(mv_obj, "targetValue").unwrap_or(&Value::Null));

                let material = match material_name {
                    Some(name) => scene.materials.iter().position(|m| m.name == name),
                    None => None,
                };
                let Some(material) = material else {
                    continue;
                };

                let has_four = target_value.len() == 4;
                if has_four {
                    if let Some(kind) = v0_material_color_kind(property) {
                        if let Some(bind) = build_v0_color_bind(material, kind, &target_value, scene) {
                            binds.push(Bind::Color(bind));
                            continue;
                        }
                    }
                }
                // else: texture transform bind
                let scale = Vec2::new(
                    *target_value.get(2).unwrap_or(&1.0),
                    *target_value.get(3).unwrap_or(&1.0),
                );
                let offset = Vec2::new(
                    *target_value.get(0).unwrap_or(&0.0),
                    1.0 - target_value.get(1).copied().unwrap_or(0.0) - target_value.get(3).copied().unwrap_or(0.0),
                );
                if scene.materials.get(material).is_some() {
                    binds.push(Bind::TextureTransform(TextureTransformBind::new(
                        material, scale, offset, scene,
                    )));
                }
            }
        }

        manager.register_expression(Expression {
            name: display_name,
            preset_name: preset_name,
            weight: 0.0,
            is_binary,
            override_blink: OverrideType::None,
            override_look_at: OverrideType::None,
            override_mouth: OverrideType::None,
            binds,
        });
    }

    manager
}

fn parse_vrm0_morph_target_bind(v: &Value, scene: &crate::scene::Scene) -> Option<MorphTargetBind> {
    let o = jobj(v)?;
    let mesh_node = get(o, "mesh").and_then(ju32)? as usize;
    let index = get(o, "index").and_then(ju32)? as usize;
    let weight = get(o, "weight").and_then(f32_of).unwrap_or(1.0);

    // VRM 0.x `mesh` is a node index.
    let mesh = scene.node(mesh_node).mesh?;
    if scene.meshes.get(mesh).is_none() {
        return None;
    }

    Some(MorphTargetBind {
        mesh,
        target: index,
        weight,
    })
}

fn build_v0_color_bind(
    material: usize,
    kind: MaterialColorKind,
    target_value: &[f32],
    scene: &crate::scene::Scene,
) -> Option<MaterialColorBind> {
    let mat = scene.materials.get(material)?;
    let (initial_rgb, initial_alpha) = current_material_color(mat, kind);
    let target_rgb = Vec3::new(target_value[0], target_value[1], target_value[2]);
    // VRM 0.x color binds do not set alpha (three.js `new THREE.Color(...targetValue)` uses 3 channels).
    let target_alpha = initial_alpha.unwrap_or(1.0);
    let delta_rgb = target_rgb - initial_rgb;
    let delta_alpha = if matches!(kind, MaterialColorKind::Color) {
        Some(target_alpha - initial_alpha.unwrap_or(1.0))
    } else {
        None
    };

    Some(MaterialColorBind {
        material,
        kind,
        initial_rgb,
        delta_rgb,
        initial_alpha: if matches!(kind, MaterialColorKind::Color) {
            initial_alpha
        } else {
            None
        },
        delta_alpha,
    })
}

fn parse_vrm0_look_at(v: &Value, humanoid: &Humanoid, scene: &crate::scene::Scene) -> Option<LookAt> {
    let o = jobj(v)?;
    let look_at_type = get(o, "lookAtTypeName").and_then(jstr).unwrap_or("Bone");
    let default_output_scale = if look_at_type == "BlendShape" { 1.0 } else { 10.0 };

    let map_hi = import_vrm0_degree_map(get(o, "lookAtHorizontalInner"), default_output_scale);
    let map_ho = import_vrm0_degree_map(get(o, "lookAtHorizontalOuter"), default_output_scale);
    let map_vd = import_vrm0_degree_map(get(o, "lookAtVerticalDown"), default_output_scale);
    let map_vu = import_vrm0_degree_map(get(o, "lookAtVerticalUp"), default_output_scale);

    let mut applier = match look_at_type {
        "BlendShape" => Applier::Expression(ExpressionApplier::new(map_hi, map_ho, map_vd, map_vu)),
        _ => Applier::Bone(BoneApplier::new(humanoid, scene, map_hi, map_ho, map_vd, map_vu)),
    };

    // VRM 0.x faces -Z; set faceFront on the bone applier.
    if let Applier::Bone(bone) = &mut applier {
        bone.face_front = Vec3::new(0.0, 0.0, -1.0);
    }

    let mut look_at = LookAt::new(humanoid, applier);

    // offsetFromHeadBone: VRM0 z is opposite.
    if let Some(offset) = get(o, "firstPersonBoneOffset").and_then(jobj) {
        look_at.offset_from_head_bone = Vec3::new(
            get(offset, "x").and_then(f32_of).unwrap_or(0.0),
            get(offset, "y").and_then(f32_of).unwrap_or(0.06),
            -(get(offset, "z").and_then(f32_of).unwrap_or(0.0)),
        );
    } else {
        look_at.offset_from_head_bone = Vec3::new(0.0, 0.06, 0.0);
    }

    Some(look_at)
}

fn import_vrm0_degree_map(v: Option<&Value>, default_output_scale: f32) -> RangeMap {
    let o = v.and_then(jobj);
    let x_range = o
        .and_then(|o| get(o, "xRange"))
        .and_then(f32_of)
        .unwrap_or(90.0)
        .max(0.01);
    let y_range = o
        .and_then(|o| get(o, "yRange"))
        .and_then(f32_of)
        .unwrap_or(default_output_scale);
    RangeMap::new(x_range, y_range)
}

fn parse_vrm0_first_person(
    v: &Value,
    scene: &crate::scene::Scene,
    head: Option<usize>,
) -> FirstPerson {
    let o = jobj(v);
    let mut annotations = Vec::new();

    for node in &scene.nodes {
        if node.mesh.is_none() {
            continue;
        }
        let flag = if let Some(o) = o {
            let node_mesh = node.mesh;
            get(o, "meshAnnotations")
                .and_then(jarr)
                .and_then(|arr| {
                    arr.iter().find_map(|a| {
                        let ao = jobj(a)?;
                        let a_mesh = get(ao, "mesh").and_then(ju32)? as usize;
                        // VRM 0.x `mesh` annotation references the glTF mesh index of the node.
                        if Some(a_mesh) == node_mesh {
                            Some(FirstPersonFlag::from_v0(get(ao, "firstPersonFlag").and_then(jstr)))
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(FirstPersonFlag::Auto)
        } else {
            FirstPersonFlag::Auto
        };

        annotations.push(MeshAnnotation {
            node: node.index,
            flag,
        });
    }

    FirstPerson::new(head, annotations)
}

// ----------------------------- material color helper -----------------------------

/// Returns the current (rgb, optional alpha) of the material for a color bind kind.
fn current_material_color(mat: &crate::material::Material, kind: MaterialColorKind) -> (Vec3, Option<f32>) {
    use MaterialColorKind::*;
    match kind {
        Color => (mat.color.truncate(), Some(mat.color.w)),
        EmissionColor => (mat.emissive, None),
        ShadeColor => (mat.shade_color, None),
        MatcapColor => (mat.matcap_color, None),
        RimColor => (mat.rim_color, None),
        OutlineColor => (mat.outline_color, None),
    }
}

// ----------------------------- spring bone (v1) -----------------------------

fn import_vrm1_spring_bone(v: &Value, _scene: &crate::scene::Scene) -> Option<SpringBoneManager> {
    let o = jobj(v)?;
    let mut manager = SpringBoneManager::new();

    // colliders
    let colliders = get(o, "colliders")
        .and_then(jarr)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| parse_vrm1_collider(c))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    manager.colliders = colliders;

    // collider groups
    if let Some(groups) = get(o, "colliderGroups").and_then(jarr) {
        for group in groups {
            let Some(g) = jobj(group) else {
                continue;
            };
            let name = get(g, "name").and_then(jstr).map(String::from);
            let colliders = get(g, "colliders")
                .and_then(jarr)
                .map(|a| a.iter().filter_map(ju32).map(|i| i as usize).collect())
                .unwrap_or_default();
            manager.collider_groups.push(ColliderGroup { colliders, name });
        }
    }

    // springs
    if let Some(springs) = get(o, "springs").and_then(jarr) {
        for spring in springs {
            let Some(s) = jobj(spring) else {
                continue;
            };

            let center = get(s, "center").and_then(ju32).map(|i| i as usize);

            let collider_groups_for_spring: Vec<usize> = get(s, "colliderGroups")
                .and_then(jarr)
                .map(|a| a.iter().filter_map(ju32).map(|i| i as usize).collect())
                .unwrap_or_default();

            let joints = get(s, "joints").and_then(jarr).unwrap_or(&[]);
            let mut prev: Option<&Value> = None;
            for joint in joints {
                if let Some(prev_joint) = prev {
                    let Some(prev_o) = jobj(prev_joint) else {
                        prev = Some(joint);
                        continue;
                    };
                    let Some(cur_o) = jobj(joint) else {
                        continue;
                    };
                    let bone = get(prev_o, "node").and_then(ju32)? as usize;
                    let child = get(cur_o, "node").and_then(ju32).map(|i| i as usize);

                    let settings = JointSettings {
                        hit_radius: get(prev_o, "hitRadius").and_then(f32_of).unwrap_or(0.0),
                        drag_force: get(prev_o, "dragForce").and_then(f32_of).unwrap_or(0.4),
                        gravity_power: get(prev_o, "gravityPower").and_then(f32_of).unwrap_or(0.0),
                        stiffness: get(prev_o, "stiffness").and_then(f32_of).unwrap_or(1.0),
                        gravity_dir: get(prev_o, "gravityDir").and_then(vec3_of).unwrap_or(Vec3::new(0.0, -1.0, 0.0)),
                    };

                    let mut joint = Joint::new(bone, child, settings, collider_groups_for_spring.clone());
                    joint.center = center;
                    manager.joints.push(joint);
                }
                prev = Some(joint);
            }
        }
    }

    Some(manager)
}

fn parse_vrm1_collider(v: &Value) -> Option<Collider> {
    let o = jobj(v)?;
    let node = get(o, "node").and_then(ju32)? as usize;
    let shape_obj = get(o, "shape")?;
    let so = jobj(shape_obj)?;

    // Check for the extended collider extension (VRMC_springBone_extended_collider).
    let extended = get(o, "extensions")
        .and_then(jobj)
        .and_then(|ext| get(ext, "VRMC_springBone_extended_collider"))
        .and_then(jobj);

    if let Some(ext) = extended {
        let ext_shape = get(ext, "shape")?;
        if let Some(ext_so) = jobj(ext_shape) {
            if let Some(sphere) = get(ext_so, "sphere") {
                let so = jobj(sphere)?;
                return Some(Collider {
                    node,
                    shape: ColliderShape::Sphere {
                        offset: get(so, "offset").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                        radius: get(so, "radius").and_then(f32_of).unwrap_or(0.0),
                        inside: get(so, "inside").and_then(jbool).unwrap_or(false),
                    },
                });
            }
            if let Some(capsule) = get(ext_so, "capsule") {
                let co = jobj(capsule)?;
                return Some(Collider {
                    node,
                    shape: ColliderShape::Capsule {
                        offset: get(co, "offset").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                        tail: get(co, "tail").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                        radius: get(co, "radius").and_then(f32_of).unwrap_or(0.0),
                        inside: get(co, "inside").and_then(jbool).unwrap_or(false),
                    },
                });
            }
            if let Some(plane) = get(ext_so, "plane") {
                let po = jobj(plane)?;
                return Some(Collider {
                    node,
                    shape: ColliderShape::Plane {
                        offset: get(po, "offset").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                        normal: get(po, "normal").and_then(vec3_of).unwrap_or(Vec3::new(0.0, 0.0, 1.0)),
                    },
                });
            }
        }
    }

    // base shape (sphere or capsule, inside=false)
    if let Some(sphere) = get(so, "sphere") {
        let so = jobj(sphere)?;
        return Some(Collider {
            node,
            shape: ColliderShape::Sphere {
                offset: get(so, "offset").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                radius: get(so, "radius").and_then(f32_of).unwrap_or(0.0),
                inside: false,
            },
        });
    }
    if let Some(capsule) = get(so, "capsule") {
        let co = jobj(capsule)?;
        return Some(Collider {
            node,
            shape: ColliderShape::Capsule {
                offset: get(co, "offset").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                tail: get(co, "tail").and_then(vec3_of).unwrap_or(Vec3::ZERO),
                radius: get(co, "radius").and_then(f32_of).unwrap_or(0.0),
                inside: false,
            },
        });
    }

    None
}

// ----------------------------- spring bone (v0) -----------------------------

fn import_vrm0_spring_bone(v: &Value, _scene: &crate::scene::Scene) -> Option<SpringBoneManager> {
    let o = jobj(v)?;
    let mut manager = SpringBoneManager::new();

    // collider groups
    if let Some(groups) = get(o, "colliderGroups").and_then(jarr) {
        for group in groups {
            let Some(g) = jobj(group) else {
                continue;
            };
            let group_node = get(g, "node").and_then(ju32)? as usize;
            let name = get(g, "name").and_then(jstr).map(String::from);
            let collider_indices: Vec<usize> = get(g, "colliders")
                .and_then(jarr)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| {
                            let co = jobj(c)?;
                            let offset = if let Some(off) = get(co, "offset").and_then(jobj) {
                                Vec3::new(
                                    get(off, "x").and_then(f32_of).unwrap_or(0.0),
                                    get(off, "y").and_then(f32_of).unwrap_or(0.0),
                                    // VRM 0.x z is opposite
                                    -(get(off, "z").and_then(f32_of).unwrap_or(0.0)),
                                )
                            } else {
                                Vec3::ZERO
                            };
                            let radius = get(co, "radius").and_then(f32_of).unwrap_or(0.0);
                            let index = manager.colliders.len();
                            manager.colliders.push(Collider {
                                node: group_node,
                                shape: ColliderShape::Sphere {
                                    offset,
                                    radius,
                                    inside: false,
                                },
                            });
                            Some(index)
                        })
                        .collect()
                })
                .unwrap_or_default();
            manager.collider_groups.push(ColliderGroup {
                colliders: collider_indices,
                name,
            });
        }
    }

    // bone groups
    if let Some(bone_groups) = get(o, "boneGroups").and_then(jarr) {
        for group in bone_groups {
            let Some(g) = jobj(group) else {
                continue;
            };

            let collider_groups_for_spring: Vec<usize> = get(g, "colliderGroups")
                .and_then(jarr)
                .map(|a| a.iter().filter_map(ju32).map(|i| i as usize).collect())
                .unwrap_or_default();

            let center = get(g, "center").and_then(ju32).map(|i| i as usize);
            let settings = JointSettings {
                hit_radius: get(g, "hitRadius").and_then(f32_of).unwrap_or(0.0),
                drag_force: get(g, "dragForce").and_then(f32_of).unwrap_or(0.4),
                gravity_power: get(g, "gravityPower").and_then(f32_of).unwrap_or(0.0),
                // VRM 0.x spec field is `stiffiness` (typo preserved in the spec)
                stiffness: get(g, "stiffiness")
                    .and_then(f32_of)
                    .or_else(|| get(g, "stiffness").and_then(f32_of))
                    .unwrap_or(1.0),
                gravity_dir: get(g, "gravityDir")
                    .and_then(jobj)
                    .and_then(|gd| {
                        Some(Vec3::new(
                            get(gd, "x").and_then(f32_of)?,
                            get(gd, "y").and_then(f32_of)?,
                            get(gd, "z").and_then(f32_of)?,
                        ))
                    })
                    .unwrap_or(Vec3::new(0.0, -1.0, 0.0)),
            };

            let roots = get(g, "bones").and_then(jarr).unwrap_or(&[]);
            let scene = _scene;
            for root in roots {
                let Some(root_index) = ju32(root) else {
                    continue;
                };
                let root_index = root_index as usize;
                // traverse all descendants of root, each becomes a joint
                let subtree = scene.subtree(root_index);
                for node_index in subtree {
                    let child = scene
                        .node(node_index)
                        .children
                        .first()
                        .copied();
                    let mut joint = Joint::new(node_index, child, settings, collider_groups_for_spring.clone());
                    joint.center = center;
                    manager.joints.push(joint);
                }
            }
        }
    }

    Some(manager)
}