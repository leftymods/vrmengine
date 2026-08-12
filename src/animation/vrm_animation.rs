use std::collections::HashMap;

pub struct VRMAnimation {
    pub duration: f32,
    pub humanoid_tracks: HumanoidTracks,
    pub expression_tracks: ExpressionTracks,
}

pub struct HumanoidTracks {
    pub translation: HashMap<String, Vec<f32>>,
    pub rotation: HashMap<String, Vec<f32>>,
}

pub struct ExpressionTracks {
    pub preset: HashMap<String, Vec<f32>>,
    pub custom: HashMap<String, Vec<f32>>,
}
