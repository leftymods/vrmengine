pub trait VRMExpressionBindTrait {
    fn apply_weight(&mut self, weight: f32);
    fn clear_applied_weight(&mut self);
}
