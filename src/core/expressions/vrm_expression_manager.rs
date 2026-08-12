use super::vrm_expression::VRMExpression;
use std::collections::HashMap;

pub struct VRMExpressionManager {
    pub expressions: Vec<VRMExpression>,
    pub expression_map: HashMap<String, usize>,
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
        let index = self.expressions.len();
        self.expression_map.insert(expression.expression_name.clone(), index);
        self.expressions.push(expression);
    }

    pub fn get_expression(&self, name: &str) -> Option<&VRMExpression> {
        self.expression_map.get(name).and_then(|&i| self.expressions.get(i))
    }

    pub fn get_value(&self, name: &str) -> Option<f32> {
        self.get_expression(name).map(|e| e.weight)
    }

    pub fn set_value(&mut self, name: &str, weight: f32) {
        if let Some(&i) = self.expression_map.get(name) {
            if let Some(e) = self.expressions.get_mut(i) {
                e.weight = weight.clamp(0.0, 1.0);
            }
        }
    }

    pub fn reset_values(&mut self) {
        for e in &mut self.expressions {
            e.weight = 0.0;
        }
    }

    pub fn update(&mut self) {
        let mut blink = 1.0f32;
        let mut look_at = 1.0f32;
        let mut mouth = 1.0f32;

        for expression in &self.expressions {
            let amount = if expression.is_binary && expression.weight > 0.5 { 1.0 } else { expression.weight };
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
