pub fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
