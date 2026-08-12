pub fn sanitize_angle(angle: f32) -> f32 { angle.clamp(-90.0, 90.0) }
