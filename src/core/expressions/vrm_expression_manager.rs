use super::VRMExpression;
use std::collections::HashMap;

pub struct VRMExpressionManager {
    pub expressions: Vec<VRMExpression>,
    pub expression_map: HashMap<String, VRMExpression>,
    pub blink_expression_names: Vec<String>,
    pub look_at_expression_names: Vec<String>,
    pub mouth_expression_names: Vec<String>,
}
