use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct VRMPose {
    pub transforms: HashMap<String, VRMPoseTransform>,
}

#[derive(Clone, Debug, Default)]
pub struct VRMPoseTransform {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
}
