#[derive(Clone, Debug, Default)]
pub struct VRMPoseTransform {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
}
