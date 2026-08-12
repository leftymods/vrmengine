use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresetName {
    Aa, Ih, Ou, Ee, Oh, Blink, Happy, Angry, Sad, Relaxed, Neutral,
    LookUp, Surprised, LookDown, LookLeft, LookRight,
    BlinkLeft, BlinkRight,
}

#[derive(Clone, Debug)]
pub struct VRMExpression {
    pub name: String,
    pub expression_name: String,
    pub weight: f32,
    pub is_binary: bool,
    pub override_blink: String,
    pub override_look_at: String,
    pub override_mouth: String,
}

#[derive(Clone, Debug, Default)]
pub struct VRMExpressionManager {
    pub expressions: Vec<VRMExpression>,
    pub expression_map: HashMap<String, VRMExpression>,
}

impl VRMExpressionManager {
    pub fn register_expression(&mut self, expr: VRMExpression) {
        self.expression_map.insert(expr.expression_name.clone(), expr.clone());
        self.expressions.push(expr);
    }
    pub fn get_value(&self, name: &str) -> Option<f32> {
        self.expression_map.get(name).map(|e| e.weight)
    }
    pub fn set_value(&mut self, name: &str, weight: f32) {
        if let Some(e) = self.expression_map.get_mut(name) {
            e.weight = weight;
        }
    }
}
