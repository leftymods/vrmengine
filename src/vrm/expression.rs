//! VRM expressions, ported from `@pixiv/three-vrm-core/expressions`.
//!
//! Contains `Expression`, the bind types (morph target / material color / texture transform),
//! and the `ExpressionManager` with blink / lookAt / mouth override logic.
//!
//! Note: the current upstream `update()` references an undefined `cachedMultiplier`; we port the
//! stable, spec-correct override logic from the documented manager implementation instead.

use std::collections::HashMap;

use glam::{Vec2, Vec3, Vec4};

use crate::material::{MaterialKind, TexSlot, UvTransform};
use crate::scene::Scene;

pub const BLINK_EXPRESSION_NAMES: &[PresetName] = &[
    PresetName::Blink,
    PresetName::BlinkLeft,
    PresetName::BlinkRight,
];
pub const LOOK_AT_EXPRESSION_NAMES: &[PresetName] = &[
    PresetName::LookLeft,
    PresetName::LookRight,
    PresetName::LookUp,
    PresetName::LookDown,
];
pub const MOUTH_EXPRESSION_NAMES: &[PresetName] = &[
    PresetName::Aa,
    PresetName::Ee,
    PresetName::Ih,
    PresetName::Oh,
    PresetName::Ou,
];

/// `VRMExpressionPresetName` in three-vrm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetName {
    Happy,
    Angry,
    Sad,
    Relaxed,
    Surprised,
    Aa,
    Ih,
    Ou,
    Ee,
    Oh,
    Blink,
    BlinkLeft,
    BlinkRight,
    LookUp,
    LookDown,
    LookLeft,
    LookRight,
    Neutral,
    Custom,
}

impl PresetName {
    pub fn from_str(s: &str) -> Option<Self> {
        use PresetName::*;
        Some(match s {
            "happy" => Happy,
            "angry" => Angry,
            "sad" => Sad,
            "relaxed" => Relaxed,
            "surprised" => Surprised,
            "aa" => Aa,
            "ih" => Ih,
            "ou" => Ou,
            "ee" => Ee,
            "oh" => Oh,
            "blink" => Blink,
            "blinkLeft" => BlinkLeft,
            "blinkRight" => BlinkRight,
            "lookUp" => LookUp,
            "lookDown" => LookDown,
            "lookLeft" => LookLeft,
            "lookRight" => LookRight,
            "neutral" => Neutral,
            "custom" => Custom,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        use PresetName::*;
        match self {
            Happy => "happy",
            Angry => "angry",
            Sad => "sad",
            Relaxed => "relaxed",
            Surprised => "surprised",
            Aa => "aa",
            Ih => "ih",
            Ou => "ou",
            Ee => "ee",
            Oh => "oh",
            Blink => "blink",
            BlinkLeft => "blinkLeft",
            BlinkRight => "blinkRight",
            LookUp => "lookUp",
            LookDown => "lookDown",
            LookLeft => "lookLeft",
            LookRight => "lookRight",
            Neutral => "neutral",
            Custom => "custom",
        }
    }
}

/// VRM 0.x preset name -> v1 preset name map (three-vrm `v0v1PresetNameMap`).
pub fn v0_preset_to_v1(s: &str) -> Option<PresetName> {
    use PresetName::*;
    Some(match s {
        "a" => Aa,
        "e" => Ee,
        "i" => Ih,
        "o" => Oh,
        "u" => Ou,
        "blink" => Blink,
        "joy" => Happy,
        "angry" => Angry,
        "sorrow" => Sad,
        "fun" => Relaxed,
        "lookup" => LookUp,
        "lookdown" => LookDown,
        "lookleft" => LookLeft,
        "lookright" => LookRight,
        "blink_l" => BlinkLeft,
        "blink_r" => BlinkRight,
        _ => return None,
    })
}

/// VRM 0.x preset names that define binary expressions.
pub fn v0_preset_is_binary(s: &str) -> bool {
    matches!(
        s,
        "blink" | "lookup" | "lookdown" | "lookleft" | "lookright" | "blink_l" | "blink_r"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideType {
    None,
    Block,
    Blend,
}

impl OverrideType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "block" => OverrideType::Block,
            "blend" => OverrideType::Blend,
            _ => OverrideType::None,
        }
    }
}

/// Material color target kinds for `VRMExpressionMaterialColorBind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialColorKind {
    Color,
    EmissionColor,
    ShadeColor,
    MatcapColor,
    RimColor,
    OutlineColor,
}

impl MaterialColorKind {
    pub fn from_str(s: &str) -> Option<Self> {
        use MaterialColorKind::*;
        Some(match s {
            "color" => Color,
            "emissionColor" => EmissionColor,
            "shadeColor" => ShadeColor,
            "matcapColor" => MatcapColor,
            "rimColor" => RimColor,
            "outlineColor" => OutlineColor,
            _ => return None,
        })
    }
}

/// Map of VRM 0.x material property names to color bind kinds.
pub fn v0_material_color_kind(property_name: &str) -> Option<MaterialColorKind> {
    use MaterialColorKind::*;
    Some(match property_name {
        "_Color" => Color,
        "_EmissionColor" => EmissionColor,
        "_ShadeColor" => ShadeColor,
        "_RimColor" => RimColor,
        "_OutlineColor" => OutlineColor,
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct MorphTargetBind {
    pub mesh: usize,
    pub target: usize,
    pub weight: f32,
}

impl MorphTargetBind {
    pub fn apply(&self, scene: &mut Scene, weight: f32) {
        if let Some(mesh) = scene.meshes.get_mut(self.mesh) {
            for primitive in &mut mesh.primitives {
                if let Some(w) = primitive.morph_weights.get_mut(self.target) {
                    *w += self.weight * weight;
                }
            }
        }
    }

    pub fn clear(&self, scene: &mut Scene) {
        if let Some(mesh) = scene.meshes.get_mut(self.mesh) {
            for primitive in &mut mesh.primitives {
                if let Some(w) = primitive.morph_weights.get_mut(self.target) {
                    *w = 0.0;
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MaterialColorBind {
    pub material: usize,
    pub kind: MaterialColorKind,
    pub initial_rgb: Vec3,
    pub delta_rgb: Vec3,
    pub initial_alpha: Option<f32>,
    pub delta_alpha: Option<f32>,
}

impl MaterialColorBind {
    /// Set `rgb` (and optionally `alpha`) field on the material for this bind's color kind.
    fn set_color(&self, material: &mut crate::material::Material, rgb: Vec3, alpha: Option<f32>) {
        use MaterialColorKind::*;
        match self.kind {
            Color => {
                let a = alpha.unwrap_or(material.color.w);
                material.color = Vec4::new(rgb.x, rgb.y, rgb.z, a);
            }
            EmissionColor => material.emissive = rgb,
            ShadeColor => material.shade_color = rgb,
            MatcapColor => material.matcap_color = rgb,
            RimColor => material.rim_color = rgb,
            OutlineColor => material.outline_color = rgb,
        }
    }

    pub fn apply(&self, scene: &mut Scene, weight: f32) {
        let Some(material) = scene.materials.get_mut(self.material) else {
            return;
        };
        let rgb = self.initial_rgb + self.delta_rgb * weight;
        let alpha = match (self.initial_alpha, self.delta_alpha) {
            (Some(initial_alpha), Some(delta_alpha)) => Some(initial_alpha + delta_alpha * weight),
            _ => None,
        };
        self.set_color(material, rgb, alpha);
    }

    pub fn clear(&self, scene: &mut Scene) {
        let Some(material) = scene.materials.get_mut(self.material) else {
            return;
        };
        self.set_color(material, self.initial_rgb, self.initial_alpha);
    }
}

/// Slots affected by a texture transform bind, depending on material kind.
///
/// Mirrors `VRMExpressionTextureTransformBind._propertyNamesMap` (map / normalMap / emissiveMap /
/// shadeMultiplyTexture / rimMultiplyTexture / outlineWidthMultiplyTexture / uvAnimationMaskTexture).
fn tex_transform_slots(kind: MaterialKind) -> Vec<TexSlot> {
    use TexSlot::*;
    match kind {
        MaterialKind::Mtoon => vec![
            BaseColor,
            Normal,
            Emissive,
            Shade,
            Rim,
            OutlineWidth,
            UvAnimMask,
        ],
        MaterialKind::Standard => vec![BaseColor, Emissive, Normal],
        MaterialKind::Unlit => vec![BaseColor],
    }
}

#[derive(Debug, Clone)]
pub struct TextureTransformBind {
    pub material: usize,
    pub scale: Vec2,
    pub offset: Vec2,
    pub slots: Vec<TexSlot>,
    pub initial: Vec<(TexSlot, UvTransform)>,
    pub delta: Vec<(TexSlot, UvTransform)>,
}

impl TextureTransformBind {
    pub fn new(material: usize, scale: Vec2, offset: Vec2, scene: &Scene) -> Self {
        let kind = scene
            .materials
            .get(material)
            .map(|m| m.kind)
            .unwrap_or(MaterialKind::Standard);
        let slots = tex_transform_slots(kind);

        // Capture per-slot initial transforms from the current material state. Like three.js,
        // the last-applied bind wins: each apply overwrites from `initial + delta * weight`.
        let mut initial = Vec::new();
        let mut delta = Vec::new();
        if let Some(mat) = scene.materials.get(material) {
            for slot in &slots {
                let init = mat.uv_transform(*slot);
                let d = [
                    scale.x - init[0],
                    scale.y - init[1],
                    offset.x - init[2],
                    offset.y - init[3],
                ];
                initial.push((*slot, init));
                delta.push((*slot, d));
            }
        }
        TextureTransformBind {
            material,
            scale,
            offset,
            slots,
            initial,
            delta,
        }
    }

    pub fn apply(&self, scene: &mut Scene, weight: f32) {
        let Some(material) = scene.materials.get_mut(self.material) else {
            return;
        };
        for (slot, delta) in &self.delta {
            let init = self
                .initial
                .iter()
                .find(|(s, _)| s == slot)
                .map(|(_, t)| *t)
                .unwrap_or([1.0, 1.0, 0.0, 0.0]);
            let t = [
                init[0] + delta[0] * weight,
                init[1] + delta[1] * weight,
                init[2] + delta[2] * weight,
                init[3] + delta[3] * weight,
            ];
            material.set_uv_transform(*slot, t);
        }
    }

    pub fn clear(&self, scene: &mut Scene) {
        let Some(material) = scene.materials.get_mut(self.material) else {
            return;
        };
        for (slot, transform) in &self.initial {
            material.set_uv_transform(*slot, *transform);
        }
    }
}

#[derive(Debug, Clone)]
pub enum Bind {
    Morph(MorphTargetBind),
    Color(MaterialColorBind),
    TextureTransform(TextureTransformBind),
}

impl Bind {
    pub fn apply(&self, scene: &mut Scene, weight: f32) {
        match self {
            Bind::Morph(b) => b.apply(scene, weight),
            Bind::Color(b) => b.apply(scene, weight),
            Bind::TextureTransform(b) => b.apply(scene, weight),
        }
    }

    pub fn clear(&self, scene: &mut Scene) {
        match self {
            Bind::Morph(b) => b.clear(scene),
            Bind::Color(b) => b.clear(scene),
            Bind::TextureTransform(b) => b.clear(scene),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Expression {
    pub name: String,
    pub preset_name: Option<PresetName>,
    pub weight: f32,
    pub is_binary: bool,
    pub override_blink: OverrideType,
    pub override_look_at: OverrideType,
    pub override_mouth: OverrideType,
    pub binds: Vec<Bind>,
}

impl Expression {
    fn clear_applied_weight(&self, scene: &mut Scene) {
        for bind in &self.binds {
            bind.clear(scene);
        }
    }

    fn apply_weight(&self, scene: &mut Scene, weight: f32) {
        for bind in &self.binds {
            bind.apply(scene, weight);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExpressionManager {
    pub expressions: Vec<Expression>,
    expression_map: HashMap<String, usize>,
}

impl ExpressionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_value(&self, name: &str) -> f32 {
        self.expression_map
            .get(name)
            .map(|i| self.expressions[*i].weight)
            .unwrap_or(0.0)
    }

    pub fn set_value(&mut self, name: &str, value: f32) -> anyhow::Result<()> {
        let index = self
            .expression_map
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!(r#"VRMExpressionManager: Expression "{name}" not found."#))?;
        let expression = &mut self.expressions[index];
        expression.weight = if expression.is_binary {
            if value > 0.5 {
                1.0
            } else {
                0.0
            }
        } else {
            value
        };
        Ok(())
    }

    pub fn set_value_with_preset(&mut self, preset_name: PresetName, value: f32) -> anyhow::Result<()> {
        let key = preset_name.as_str().to_string();
        let index = self
            .expression_map
            .get(&key)
            .copied()
            .ok_or_else(|| anyhow::anyhow!(r#"VRMExpressionManager: Expression "{key}" not found."#))?;
        let expression = &mut self.expressions[index];
        expression.weight = if expression.is_binary && value > 0.0 {
            1.0
        } else {
            value
        };
        Ok(())
    }

    pub fn register_expression(&mut self, expression: Expression) {
        self.expression_map
            .insert(expression.name.clone(), self.expressions.len());
        self.expressions.push(expression);

        // Sort so that `block`-overriding expressions come first (matches `registerExpression`).
        self.expressions.sort_by(|a, b| {
            let a_block = (
                a.override_blink == OverrideType::Block,
                a.override_look_at == OverrideType::Block,
                a.override_mouth == OverrideType::Block,
            );
            let b_block = (
                b.override_blink == OverrideType::Block,
                b.override_look_at == OverrideType::Block,
                b.override_mouth == OverrideType::Block,
            );
            b_block.cmp(&a_block)
        });

        // Rebuild the map after sorting (stable sort keeps order for equal keys).
        self.expression_map.clear();
        for (i, expression) in self.expressions.iter().enumerate() {
            self.expression_map.insert(expression.name.clone(), i);
        }
    }

    pub fn update(&mut self, scene: &mut Scene) {
        let blink_multiply =
            Self::calc_override_multiplier(&self.expressions, BLINK_EXPRESSION_NAMES, |e| {
                e.override_blink
            });
        let look_at_multiply = Self::calc_override_multiplier(
            &self.expressions,
            LOOK_AT_EXPRESSION_NAMES,
            |e| e.override_look_at,
        );
        let mouth_multiply =
            Self::calc_override_multiplier(&self.expressions, MOUTH_EXPRESSION_NAMES, |e| {
                e.override_mouth
            });

        for expression in &self.expressions {
            expression.clear_applied_weight(scene);

            let weight = expression.weight;
            if weight == 0.0 {
                continue;
            }

            let multiplier = match expression.preset_name {
                Some(preset) if BLINK_EXPRESSION_NAMES.contains(&preset) => blink_multiply,
                Some(preset) if LOOK_AT_EXPRESSION_NAMES.contains(&preset) => look_at_multiply,
                Some(preset) if MOUTH_EXPRESSION_NAMES.contains(&preset) => mouth_multiply,
                _ => 1.0,
            };

            if multiplier == 0.0 {
                continue;
            }

            expression.apply_weight(scene, weight * multiplier);
        }
    }

    fn calc_override_multiplier(
        expressions: &[Expression],
        names: &[PresetName],
        getter: impl Fn(&Expression) -> OverrideType,
    ) -> f32 {
        let mut result = 1.0;
        for expression in expressions {
            let Some(preset) = expression.preset_name else {
                continue;
            };
            if names.contains(&preset) {
                continue;
            }
            match getter(expression) {
                OverrideType::Block => result = 0.0,
                OverrideType::Blend if expression.weight != 0.0 => {
                    result *= 1.0 - expression.weight;
                }
                _ => {}
            }
        }
        result
    }
}
