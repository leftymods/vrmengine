use super::{PresetName, VRMExpressionPresetName};

pub enum OverrideType {
    None,
    Block,
    Blend,
}

pub struct VRMExpressionBind;

pub trait VRMExpressionBindTrait {
    fn apply_weight(&mut self, weight: f32);
    fn clear_applied_weight(&mut self);
}

pub struct VRMExpression {
    pub expression_name: String,
    pub weight: f32,
    pub is_binary: bool,
    pub override_blink: OverrideType,
    pub override_look_at: OverrideType,
    pub override_mouth: OverrideType,
    pub binds: Vec<Boxdyn VRMExpressionBindTrait>>,
}

impl VRMExpression {
    pub fn new(name: &str) -> Self {
        Self {
            expression_name: name.to_string(),
            weight: 0.0,
            is_binary: false,
            override_blink: OverrideType::None,
            override_look_at: OverrideType::None,
            override_mouth: OverrideType::None,
            binds: vec![],
        }
    }

    pub fn add_bind(&mut self, bind: Boxdyn VRMExpressionBindTrait>) {
        self.binds.push(bind);
    }

    pub fn apply_weight(&mut self, multiplier: Option<f32>) {
        let mut actual_weight = self.output_weight();
        actual_weight *= multiplier.unwrap_or(1.0);
        if self.is_binary && actual_weight < 1.0 {
            actual_weight = 0.0;
        }
        for bind in &mut self.binds {
            bind.apply_weight(actual_weight);
        }
    }

    pub fn clear_applied_weight(&mut self) {
        for bind in &mut self.binds {
            bind.clear_applied_weight();
        }
    }

    fn output_weight(&self) -> f32 {
        if self.is_binary {
            if self.weight > 0.5 { 1.0 } else { 0.0 }
        } else {
            self.weight
        }
    }
}
