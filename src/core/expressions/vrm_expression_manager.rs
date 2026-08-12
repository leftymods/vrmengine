use super::VRMExpression;
use std::collections::HashMap;

pub struct VRMExpressionManager {
    pub expressions: Vec<VRMExpression>,
    pub expression_map: HashMap<String, VRMExpression>,
    pub blink_expression_names: Vec<String>,
    pub look_at_expression_names: Vec<String>,
    pub mouth_expression_names: Vec<String>,
}

impl VRMExpressionManager {
    pub fn new() -> Self {
        Self {
            expressions: vec![],
            expression_map: HashMap::new(),
            blink_expression_names: vec!["blink".to_string(), "blinkLeft".to_string(), "blinkRight".to_string()],
            look_at_expression_names: vec!["lookLeft".to_string(), "lookRight".to_string(), "lookUp".to_string(), "lookDown".to_string()],
            mouth_expression_names: vec!["aa".to_string(), "ee".to_string(), "ih".to_string(), "oh".to_string(), "ou".to_string()],
        }
    }

    pub fn register_expression(&mut self, expression: VRMExpression) {
        self.expression_map.insert(expression.expression_name.clone(), expression.clone());
        self.expressions.push(expression);
    }

    pub fn get_expression(&self, name: &str) -> OptionVRMExpression> {
        self.expression_map.get(name).cloned()
    }

    pub fn get_value(&self, name: &str) -> Option<f32> {
        self.get_expression(name).map(|e| e.weight)
    }

    pub fn set_value(&mut self, name: &str, weight: f32) {
        if let Some(e) = self.expression_map.get_mut(name) {
            e.weight = weight.clamp(0.0, 1.0);
        }
    }

    pub fn reset_values(&mut self) {
        for e in &mut self.expressions {
            e.weight = 0.0;
        }
    }

    pub fn update(&mut self) {
        // Calculate weight multipliers based on overrides
        let mut blink = 1.0f32;
        let mut look_at = 1.0f32;
        let mut mouth = 1.0f32;

        for expression in &self.expressions {
            // Simplified override logic matching three-vrm
            let amount = if expression.is_binary && expression.weight > 0.5 { 1.0 } else { expression.weight };
            // For simplicity, treat all expressions as possible overrides based on name
            if self.blink_expression_names.contains(&expression.expression_name) {
                blink -= amount;
            }
            if self.look_at_expression_names.contains(&expression.expression_name) {
                look_at -= amount;
            }
            if self.mouth_expression_names.contains(&expression.expression_name) {
                mouth -= amount;
            }
        }
        blink = blink.max(0.0);
        look_at = look_at.max(0.0);
        mouth = mouth.max(0.0);

        // Apply weights with multipliers
        for expression in &mut self.expressions {
            let mut multiplier = 1.0f32;
            let name = &expression.expression_name;
            if self.blink_expression_names.contains(name) {
                multiplier *= blink;
            }
            if self.look_at_expression_names.contains(name) {
                multiplier *= look_at;
            }
            if self.mouth_expression_names.contains(name) {
                multiplier *= mouth;
            }
            expression.apply_weight(Some(multiplier));
        }
    }
}
