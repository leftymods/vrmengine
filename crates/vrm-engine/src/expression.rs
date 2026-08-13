//! Expressions (blend shapes) for VRM 0.0 and VRM 1.0.
//!
//! A VRM expression is a named set of morph target, material color and texture
//! transform bindings. The [`ExpressionManager`] keeps a weight per expression
//! and can accumulate the resulting morph target weights per node.

use std::collections::HashMap;

use vrm_spec::vrm_0_0;
use vrm_spec::vrmc_vrm_1_0::{self, ExpressionPresetName};

/// Unified expression preset names across VRM 0.0 and VRM 1.0.
///
/// VRM 1.0 names are canonical; VRM 0.0 names are mapped onto them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpressionPreset {
    Aa,
    Angry,
    Blink,
    BlinkLeft,
    BlinkRight,
    Ee,
    Happy,
    Ih,
    LookDown,
    LookLeft,
    LookRight,
    LookUp,
    Neutral,
    Oh,
    Ou,
    Relaxed,
    Sad,
    Surprised,
}

impl ExpressionPreset {
    pub fn name(&self) -> &'static str {
        use ExpressionPreset::*;
        match self {
            Aa => "aa",
            Angry => "angry",
            Blink => "blink",
            BlinkLeft => "blinkLeft",
            BlinkRight => "blinkRight",
            Ee => "ee",
            Happy => "happy",
            Ih => "ih",
            LookDown => "lookDown",
            LookLeft => "lookLeft",
            LookRight => "lookRight",
            LookUp => "lookUp",
            Neutral => "neutral",
            Oh => "oh",
            Ou => "ou",
            Relaxed => "relaxed",
            Sad => "sad",
            Surprised => "surprised",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        use ExpressionPreset::*;
        Some(match name {
            "aa" => Aa,
            "angry" => Angry,
            "blink" => Blink,
            "blinkLeft" => BlinkLeft,
            "blinkRight" => BlinkRight,
            "ee" => Ee,
            "happy" => Happy,
            "ih" => Ih,
            "lookDown" => LookDown,
            "lookLeft" => LookLeft,
            "lookRight" => LookRight,
            "lookUp" => LookUp,
            "neutral" => Neutral,
            "oh" => Oh,
            "ou" => Ou,
            "relaxed" => Relaxed,
            "sad" => Sad,
            "surprised" => Surprised,
            _ => return None,
        })
    }

    pub fn from_vrm1(name: ExpressionPresetName) -> Option<Self> {
        use ExpressionPreset as P;
        use ExpressionPresetName as N;
        Some(match name {
            N::Aa => P::Aa,
            N::Angry => P::Angry,
            N::Blink => P::Blink,
            N::BlinkLeft => P::BlinkLeft,
            N::BlinkRight => P::BlinkRight,
            N::Ee => P::Ee,
            N::Happy => P::Happy,
            N::Ih => P::Ih,
            N::LookDown => P::LookDown,
            N::LookLeft => P::LookLeft,
            N::LookRight => P::LookRight,
            N::LookUp => P::LookUp,
            N::Neutral => P::Neutral,
            N::Oh => P::Oh,
            N::Ou => P::Ou,
            N::Relaxed => P::Relaxed,
            N::Sad => P::Sad,
            N::Surprised => P::Surprised,
        })
    }

    /// Convert a VRM 0.0 preset name. `Unknown` and other unmappable names map
    /// to `None`.
    pub fn from_vrm0(name: vrm_0_0::PresetName) -> Option<Self> {
        use ExpressionPreset as P;
        use vrm_0_0::PresetName as N;
        Some(match name {
            N::A => P::Aa,
            N::Angry => P::Angry,
            N::Blink => P::Blink,
            N::BlinkL => P::BlinkLeft,
            N::BlinkR => P::BlinkRight,
            N::E => P::Ee,
            N::Fun => P::Happy,
            N::I => P::Ih,
            N::Joy => P::Relaxed,
            N::Lookdown => P::LookDown,
            N::Lookleft => P::LookLeft,
            N::Lookright => P::LookRight,
            N::Lookup => P::LookUp,
            N::Neutral => P::Neutral,
            N::O => P::Oh,
            N::Sorrow => P::Sad,
            N::U => P::Ou,
            N::Unknown => return None,
        })
    }
}

impl std::fmt::Display for ExpressionPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Identifier of an expression inside a model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExpressionId {
    /// A predefined preset expression.
    Preset(ExpressionPreset),
    /// A model-specific expression, identified by its name.
    Custom(String),
}

impl ExpressionId {
    pub fn name(&self) -> &str {
        match self {
            ExpressionId::Preset(p) => p.name(),
            ExpressionId::Custom(name) => name,
        }
    }

    /// Look up a preset expression by its canonical name.
    pub fn preset(name: &str) -> Option<ExpressionId> {
        ExpressionPreset::from_name(name).map(ExpressionId::Preset)
    }
}

impl std::fmt::Display for ExpressionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// How an expression affects other expressions of the same category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideType {
    /// The expression does not affect other expressions of the same category.
    None,
    /// The expression suppresses other expressions of the same category.
    Block,
    /// The expression is mixed with other expressions of the same category.
    Blend,
}

/// A single morph target binding of an expression.
#[derive(Debug, Clone, Copy)]
pub struct MorphTargetBind {
    /// Runtime node index of the mesh to apply the weight to.
    pub node: usize,
    /// Index of the morph target inside the node's mesh.
    pub index: usize,
    /// Weight multiplier of this binding.
    pub weight: f32,
}

/// A single material color binding of an expression.
#[derive(Debug, Clone)]
pub struct MaterialColorBind {
    /// Resolved glTF material index, when available.
    pub material: Option<usize>,
    /// Material name (VRM 0.0 style reference).
    pub material_name: Option<String>,
    /// Property name, e.g. `color`, `emissionColor`, `_Color`.
    pub property: String,
    /// Target value of the property (e.g. RGBA).
    pub target_value: Vec<f32>,
}

/// A single texture transform binding of an expression (VRM 1.0 only).
#[derive(Debug, Clone, Copy)]
pub struct TextureTransformBind {
    /// Resolved glTF material index.
    pub material: Option<usize>,
    /// UV offset.
    pub offset: Option<[f32; 2]>,
    /// UV scale.
    pub scale: Option<[f32; 2]>,
}

/// A runtime expression definition.
#[derive(Debug, Clone)]
pub struct Expression {
    pub id: ExpressionId,
    /// Binary expressions snap to 0 or 1.
    pub is_binary: bool,
    pub override_blink: OverrideType,
    pub override_look_at: OverrideType,
    pub override_mouth: OverrideType,
    pub morph_binds: Vec<MorphTargetBind>,
    pub material_binds: Vec<MaterialColorBind>,
    pub texture_transform_binds: Vec<TextureTransformBind>,
}

impl Expression {
    fn new(id: ExpressionId) -> Self {
        Self {
            id,
            is_binary: false,
            override_blink: OverrideType::None,
            override_look_at: OverrideType::None,
            override_mouth: OverrideType::None,
            morph_binds: Vec::new(),
            material_binds: Vec::new(),
            texture_transform_binds: Vec::new(),
        }
    }

    /// Build from a VRM 1.0 expression.
    fn from_vrm1(
        id: ExpressionId,
        expr: &vrmc_vrm_1_0::Expression,
        doc: &gltf::Document,
    ) -> Self {
        let mut out = Self::new(id);
        out.is_binary = expr.is_binary.unwrap_or(false);
        out.override_blink = OverrideType::from_vrm1(expr.override_blink);
        out.override_look_at = OverrideType::from_vrm1(expr.override_look_at);
        out.override_mouth = OverrideType::from_vrm1(expr.override_mouth);

        if let Some(binds) = &expr.morph_target_binds {
            for bind in binds {
                let node = bind.node.value();
                if node < doc.nodes().len() {
                    out.morph_binds.push(MorphTargetBind {
                        node,
                        index: bind.index,
                        weight: bind.weight as f32,
                    });
                }
            }
        }

        if let Some(binds) = &expr.material_color_binds {
            for bind in binds {
                out.material_binds.push(MaterialColorBind {
                    material: Some(bind.material.value()),
                    material_name: None,
                    property: vrm1_material_color_property(bind.r#type),
                    target_value: bind.target_value.iter().map(|&v| v as f32).collect(),
                });
            }
        }

        if let Some(binds) = &expr.texture_transform_binds {
            for bind in binds {
                out.texture_transform_binds.push(TextureTransformBind {
                    material: Some(bind.material.value()),
                    offset: bind.offset.as_deref().map(|o| [o[0] as f32, o[1] as f32]),
                    scale: bind.scale.as_deref().map(|s| [s[0] as f32, s[1] as f32]),
                });
            }
        }

        out
    }

    /// Build from a VRM 0.0 blend shape group.
    fn from_vrm0(
        id: ExpressionId,
        group: &vrm_0_0::VRMBlendShapeGroup,
        doc: &gltf::Document,
    ) -> Self {
        let mut out = Self::new(id);
        out.is_binary = group.is_binary.unwrap_or(false);

        if let Some(binds) = &group.binds {
            for bind in binds {
                let Some(mesh_index) = bind.mesh.map(|m| m.value()) else {
                    continue;
                };
                let Some(index) = bind.index else {
                    continue;
                };
                if index < 0 {
                    continue;
                }
                let weight = bind.weight.unwrap_or(1.0) as f32;
                for node in doc.nodes() {
                    if node.mesh().map(|m| m.index()) == Some(mesh_index) {
                        out.morph_binds.push(MorphTargetBind {
                            node: node.index(),
                            index: index as usize,
                            weight,
                        });
                    }
                }
            }
        }

        if let Some(values) = &group.material_values {
            for value in values {
                let material_name = value.material_name.clone();
                let material = doc
                    .materials()
                    .find(|m| m.name() == material_name.as_deref())
                    .and_then(|m| m.index());
                out.material_binds.push(MaterialColorBind {
                    material,
                    material_name,
                    property: value.property_name.clone().unwrap_or_default(),
                    target_value: value
                        .target_value
                        .as_deref()
                        .map(|v| v.iter().map(|&x| x as f32).collect())
                        .unwrap_or_default(),
                });
            }
        }

        out
    }
}

fn vrm1_material_color_property(kind: vrmc_vrm_1_0::MaterialColorType) -> String {
    use vrmc_vrm_1_0::MaterialColorType::*;
    match kind {
        Color => "color",
        EmissionColor => "emissionColor",
        MatcapColor => "matcapColor",
        OutlineColor => "outlineColor",
        RimColor => "rimColor",
        ShadeColor => "shadeColor",
    }
    .to_string()
}

impl OverrideType {
    fn from_vrm1(value: Option<vrmc_vrm_1_0::ExpressionOverrideType>) -> Self {
        use vrmc_vrm_1_0::ExpressionOverrideType as T;
        match value {
            Some(T::Block) => OverrideType::Block,
            Some(T::Blend) => OverrideType::Blend,
            _ => OverrideType::None,
        }
    }
}

/// Preset categories that take part in override resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Blink,
    LookAt,
    Mouth,
}

fn category_of(preset: ExpressionPreset) -> Option<Category> {
    use ExpressionPreset as P;
    Some(match preset {
        P::Blink | P::BlinkLeft | P::BlinkRight => Category::Blink,
        P::LookUp | P::LookDown | P::LookLeft | P::LookRight => Category::LookAt,
        P::Aa | P::Ih | P::Ou | P::Ee | P::Oh => Category::Mouth,
        _ => return None,
    })
}

/// Manager of all expressions of a model and their current weights.
#[derive(Debug, Clone, Default)]
pub struct ExpressionManager {
    expressions: Vec<Expression>,
    weights: Vec<f32>,
    index: HashMap<ExpressionId, usize>,
}

impl ExpressionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an expression. The expression starts with weight 0.
    pub fn add(&mut self, expression: Expression) {
        self.index.insert(expression.id.clone(), self.expressions.len());
        self.weights.push(0.0);
        self.expressions.push(expression);
    }

    pub fn len(&self) -> usize {
        self.expressions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.expressions.is_empty()
    }

    pub fn get(&self, id: &ExpressionId) -> Option<&Expression> {
        self.index.get(id).map(|&i| &self.expressions[i])
    }

    pub fn expressions(&self) -> &[Expression] {
        &self.expressions
    }

    pub fn ids(&self) -> impl Iterator<Item = &ExpressionId> {
        self.expressions.iter().map(|e| &e.id)
    }

    pub fn contains(&self, id: &ExpressionId) -> bool {
        self.index.contains_key(id)
    }

    /// Set the weight of an expression, clamped to `[0, 1]`.
    pub fn set_weight(&mut self, id: &ExpressionId, weight: f32) {
        if let Some(&i) = self.index.get(id) {
            self.weights[i] = weight.clamp(0.0, 1.0);
        }
    }

    pub fn weight(&self, id: &ExpressionId) -> f32 {
        self.index.get(id).map(|&i| self.weights[i]).unwrap_or(0.0)
    }

    /// Reset all expression weights to 0.
    pub fn reset(&mut self) {
        for w in &mut self.weights {
            *w = 0.0;
        }
    }

    /// Current effective weight of an expression, taking `is_binary` into
    /// account.
    pub fn effective_weight(&self, index: usize) -> f32 {
        let raw = self.weights[index].clamp(0.0, 1.0);
        if self.expressions[index].is_binary {
            if raw >= 0.5 {
                1.0
            } else {
                0.0
            }
        } else {
            raw
        }
    }

    /// Accumulate the morph target weights of all enabled expressions.
    ///
    /// `node_morph_count[i]` must be the number of morph targets of the mesh
    /// attached to node `i`. The result is one entry per node; entries are
    /// empty for nodes without morphs.
    pub fn compute_morph_weights(&self, node_morph_count: &[usize]) -> Vec<Vec<f32>> {
        let mut out: Vec<Vec<f32>> = node_morph_count.iter().map(|&n| vec![0.0; n]).collect();

        // Determine which categories have active override expressions.
        let mut suppress = [false; 3];
        for (expr, i) in self.expressions.iter().zip(0..) {
            if self.effective_weight(i) <= 0.0 {
                continue;
            }
            let category = expr.id.preset_kind().and_then(category_of);
            if let Some(cat) = category {
                let override_active = match cat {
                    Category::Blink => expr.override_blink != OverrideType::None,
                    Category::LookAt => expr.override_look_at != OverrideType::None,
                    Category::Mouth => expr.override_mouth != OverrideType::None,
                };
                if override_active {
                    suppress[cat as usize] = true;
                }
            }
        }

        for (expr, i) in self.expressions.iter().zip(0..) {
            let mut weight = self.effective_weight(i);
            if weight > 0.0 {
                let category = expr.id.preset_kind().and_then(category_of);
                if let Some(cat) = category {
                    let none = match cat {
                        Category::Blink => expr.override_blink == OverrideType::None,
                        Category::LookAt => expr.override_look_at == OverrideType::None,
                        Category::Mouth => expr.override_mouth == OverrideType::None,
                    };
                    if none && suppress[cat as usize] {
                        weight = 0.0;
                    }
                }
            }
            if weight <= 0.0 {
                continue;
            }
            for bind in &expr.morph_binds {
                if let Some(slot) = out.get_mut(bind.node) {
                    if let Some(value) = slot.get_mut(bind.index) {
                        *value += weight * bind.weight;
                    }
                }
            }
        }

        out
    }

    /// Accumulate weighted material color targets.
    ///
    /// Returns a map from material index to the accumulated target value.
    pub fn compute_material_colors(&self) -> HashMap<usize, Vec<f32>> {
        let mut out: HashMap<usize, Vec<f32>> = HashMap::new();
        for (expr, i) in self.expressions.iter().zip(0..) {
            let weight = self.effective_weight(i);
            if weight <= 0.0 {
                continue;
            }
            for bind in &expr.material_binds {
                let Some(material) = bind.material else {
                    continue;
                };
                let entry = out.entry(material).or_insert_with(|| vec![0.0; bind.target_value.len()]);
                for (dst, src) in entry.iter_mut().zip(&bind.target_value) {
                    *dst += weight * src;
                }
            }
        }
        out
    }
}

impl ExpressionId {
    fn preset_kind(&self) -> Option<ExpressionPreset> {
        match self {
            ExpressionId::Preset(p) => Some(*p),
            ExpressionId::Custom(_) => None,
        }
    }
}

/// Load all expressions of a VRM 1.0 model.
pub(crate) fn load_expressions_vrm1(
    schema: &vrmc_vrm_1_0::VRMCVrmSchema,
    doc: &gltf::Document,
) -> ExpressionManager {
    let mut manager = ExpressionManager::new();
    let Some(expressions) = &schema.expressions else {
        return manager;
    };
    if let Some(preset) = &expressions.preset {
        for (name, expr) in preset.0.iter() {
            if let Some(preset_id) = ExpressionPreset::from_vrm1(*name) {
                manager.add(Expression::from_vrm1(
                    ExpressionId::Preset(preset_id),
                    expr,
                    doc,
                ));
            }
        }
    }
    if let Some(custom) = &expressions.custom {
        for (name, expr) in custom {
            manager.add(Expression::from_vrm1(
                ExpressionId::Custom(name.clone()),
                expr,
                doc,
            ));
        }
    }
    manager
}

/// Load all expressions (blend shape groups) of a VRM 0.0 model.
pub(crate) fn load_expressions_vrm0(
    schema: &vrm_0_0::VRM0Schema,
    doc: &gltf::Document,
) -> ExpressionManager {
    let mut manager = ExpressionManager::new();
    let Some(blend_shape) = &schema.blend_shape_master else {
        return manager;
    };
    let Some(groups) = &blend_shape.blend_shape_groups else {
        return manager;
    };
    for group in groups {
        let id = match group.preset_name {
            Some(preset) => match ExpressionPreset::from_vrm0(preset) {
                Some(p) => ExpressionId::Preset(p),
                None => ExpressionId::Custom(
                    group.name.clone().unwrap_or_else(|| "custom".to_string()),
                ),
            },
            None => ExpressionId::Custom(group.name.clone().unwrap_or_else(|| "custom".to_string())),
        };
        manager.add(Expression::from_vrm0(id, group, doc));
    }
    manager
}
