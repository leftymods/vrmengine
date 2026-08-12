use crate::core::humanoid::vrm_humanoid::VRMHumanoid;
use glam::Quat;

pub struct VRMLookAt {
    pub yaw_target_degrees: f32,
    pub pitch_target_degrees: f32,
    pub range_map_horizontal_inner: Option<Vec<(f32, f32)>>,
    pub range_map_horizontal_outer: Option<Vec<(f32, f32)>>,
    pub range_map_vertical_down: Option<Vec<(f32, f32)>>,
    pub range_map_vertical_up: Option<Vec<(f32, f32)>>,
}

impl VRMLookAt {
    pub fn new() -> Self {
        Self {
            yaw_target_degrees: 0.0,
            pitch_target_degrees: 0.0,
            range_map_horizontal_inner: None,
            range_map_horizontal_outer: None,
            range_map_vertical_down: None,
            range_map_vertical_up: None,
        }
    }

    pub fn setup(&mut self, humanoid: &VRMHumanoid) {
        let _ = humanoid;
    }

    pub fn apply(&mut self, humanoid: &mut VRMHumanoid, yaw: f32, pitch: f32) {
        self.yaw_target_degrees = yaw;
        self.pitch_target_degrees = pitch;

        let yaw_rad = yaw.to_radians();
        let pitch_rad = pitch.to_radians();
        let yaw_rot = Quat::from_rotation_y(yaw_rad);
        let pitch_rot = Quat::from_rotation_x(pitch_rad);
        let combined = pitch_rot * yaw_rot;

        for bone_name in ["leftEye", "rightEye"] {
            if let Some(bone) = humanoid.normalized_human_bones.human_bones.get_mut(bone_name) {
                bone.node.quaternion = combined * bone.node.quaternion;
            }
        }
    }
}
