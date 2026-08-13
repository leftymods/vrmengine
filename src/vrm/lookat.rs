//! VRM look-at control, ported from `@pixiv/three-vrm-core/lookAt`.
//!
//! `LookAt` tracks a target world position and computes a yaw/pitch that is applied either to the
//! eye bones (`BoneApplier`) or to the look expressions (`ExpressionApplier`).

use glam::{Quat, Vec3};

use crate::scene::Scene;
use crate::vrm::expression::ExpressionManager;
use crate::vrm::humanoid::{HumanBoneName, Humanoid};

pub const RAD2DEG: f32 = 180.0 / std::f32::consts::PI;
pub const DEG2RAD: f32 = std::f32::consts::PI / 180.0;

/// Calculate azimuth / altitude angles from a vector (three.js `calcAzimuthAltitude`).
///
/// Azimuth is around the Y axis, altitude around Z, rotated in intrinsic Y-Z order.
pub fn calc_azimuth_altitude(vector: Vec3) -> (f32, f32) {
    (
        (-vector.z).atan2(vector.x),
        vector.y.atan2((vector.x * vector.x + vector.z * vector.z).sqrt()),
    )
}

/// Make sure the angle is within -PI to PI (three.js `sanitizeAngle`).
pub fn sanitize_angle(angle: f32) -> f32 {
    let round_turn = (angle / (2.0 * std::f32::consts::PI)).round();
    angle - 2.0 * std::f32::consts::PI * round_turn
}

fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn quat_from_euler_yx(yaw: f32, pitch: f32) -> Quat {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
}

/// A range map that maps an input angle to an output value (three.js `VRMLookAtRangeMap`).
#[derive(Debug, Clone, Copy)]
pub struct RangeMap {
    pub input_max_value: f32,
    pub output_scale: f32,
}

impl RangeMap {
    pub fn new(input_max_value: f32, output_scale: f32) -> Self {
        RangeMap {
            input_max_value,
            output_scale,
        }
    }

    pub fn map(&self, src: f32) -> f32 {
        self.output_scale * saturate(src / self.input_max_value)
    }
}

/// A look-at applier that rotates the eye bones (three.js `VRMLookAtBoneApplier`).
#[derive(Debug, Clone)]
pub struct BoneApplier {
    pub range_map_horizontal_inner: RangeMap,
    pub range_map_horizontal_outer: RangeMap,
    pub range_map_vertical_down: RangeMap,
    pub range_map_vertical_up: RangeMap,
    /// The front direction of the face. Set to (0, 0, -1) for VRM 0.x models.
    pub face_front: Vec3,

    left_eye: Option<usize>,
    right_eye: Option<usize>,
    left_eye_normalized: Option<usize>,
    right_eye_normalized: Option<usize>,
    rest_quat_left_eye: Quat,
    rest_quat_right_eye: Quat,
    rest_left_eye_parent_world_quat: Quat,
    rest_right_eye_parent_world_quat: Quat,
}

impl BoneApplier {
    pub fn new(
        humanoid: &Humanoid,
        scene: &Scene,
        range_map_horizontal_inner: RangeMap,
        range_map_horizontal_outer: RangeMap,
        range_map_vertical_down: RangeMap,
        range_map_vertical_up: RangeMap,
    ) -> Self {
        let left_eye = humanoid.get_raw_bone_node(HumanBoneName::LeftEye);
        let right_eye = humanoid.get_raw_bone_node(HumanBoneName::RightEye);
        let left_eye_normalized = humanoid.get_normalized_bone_node(HumanBoneName::LeftEye);
        let right_eye_normalized = humanoid.get_normalized_bone_node(HumanBoneName::RightEye);

        let rest_quat_left_eye = left_eye
            .map(|i| scene.node(i).rotation)
            .unwrap_or(Quat::IDENTITY);
        let rest_quat_right_eye = right_eye
            .map(|i| scene.node(i).rotation)
            .unwrap_or(Quat::IDENTITY);

        let rest_left_eye_parent_world_quat = left_eye
            .and_then(|i| scene.node(i).parent)
            .map(|p| scene.node_world_quaternion(p))
            .unwrap_or(Quat::IDENTITY);
        let rest_right_eye_parent_world_quat = right_eye
            .and_then(|i| scene.node(i).parent)
            .map(|p| scene.node_world_quaternion(p))
            .unwrap_or(Quat::IDENTITY);

        BoneApplier {
            range_map_horizontal_inner,
            range_map_horizontal_outer,
            range_map_vertical_down,
            range_map_vertical_up,
            face_front: Vec3::new(0.0, 0.0, 1.0),
            left_eye,
            right_eye,
            left_eye_normalized,
            right_eye_normalized,
            rest_quat_left_eye,
            rest_quat_right_eye,
            rest_left_eye_parent_world_quat,
            rest_right_eye_parent_world_quat,
        }
    }

    /// A quaternion that rotates the world-space +Z unit vector to the `face_front` direction.
    fn world_face_front_quat(&self) -> Quat {
        if self.face_front.distance_squared(Vec3::Z) < 0.01 {
            return Quat::IDENTITY;
        }
        let (azimuth, altitude) = calc_azimuth_altitude(self.face_front);
        // Euler order 'YZX': q = qy(0.5PI + azimuth) * qz(altitude)
        Quat::from_rotation_y(0.5 * std::f32::consts::PI + azimuth)
            * Quat::from_rotation_z(altitude)
    }

    /// Apply the input angle to the eye bones (three.js `VRMLookAtBoneApplier.applyYawPitch`).
    pub fn apply_yaw_pitch(&mut self, scene: &mut Scene, yaw: f32, pitch: f32) {
        let front_quat = self.world_face_front_quat();

        if let (Some(eye), Some(eye_normalized)) = (self.left_eye, self.left_eye_normalized) {
            let euler_y = if yaw < 0.0 {
                -DEG2RAD * self.range_map_horizontal_inner.map(-yaw)
            } else {
                DEG2RAD * self.range_map_horizontal_outer.map(yaw)
            };
            let euler_x = if pitch < 0.0 {
                -DEG2RAD * self.range_map_vertical_down.map(-pitch)
            } else {
                DEG2RAD * self.range_map_vertical_up.map(pitch)
            };
            self.apply_eye(
                scene,
                eye,
                eye_normalized,
                euler_y,
                euler_x,
                front_quat,
                self.rest_left_eye_parent_world_quat,
                self.rest_quat_left_eye,
            );
        }

        if let (Some(eye), Some(eye_normalized)) = (self.right_eye, self.right_eye_normalized) {
            let euler_y = if yaw < 0.0 {
                -DEG2RAD * self.range_map_horizontal_outer.map(-yaw)
            } else {
                DEG2RAD * self.range_map_horizontal_inner.map(yaw)
            };
            let euler_x = if pitch < 0.0 {
                -DEG2RAD * self.range_map_vertical_down.map(-pitch)
            } else {
                DEG2RAD * self.range_map_vertical_up.map(pitch)
            };
            self.apply_eye(
                scene,
                eye,
                eye_normalized,
                euler_y,
                euler_x,
                front_quat,
                self.rest_right_eye_parent_world_quat,
                self.rest_quat_right_eye,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_eye(
        &self,
        scene: &mut Scene,
        eye: usize,
        eye_normalized: usize,
        euler_y: f32,
        euler_x: f32,
        front_quat: Quat,
        rest_parent_world_quat: Quat,
        rest_quat: Quat,
    ) {
        // LookAt rotation in the front-facing coordinate system.
        let look_quat = quat_from_euler_yx(euler_y, euler_x);

        // world-relative look rotation: _quatB * _quatA * _quatB^-1
        let normalized_quat = front_quat * look_quat * front_quat.inverse();

        // set the normalized rig eye rotation
        let node = scene.node_mut(eye_normalized);
        node.rotation = normalized_quat;
        node.update_matrix();

        // _quatA^-1 * normalized * _quatA * restQuat
        let raw_quat = rest_parent_world_quat.inverse()
            * normalized_quat
            * rest_parent_world_quat
            * rest_quat;
        let node = scene.node_mut(eye);
        node.rotation = raw_quat;
        node.update_matrix();
    }
}

/// A look-at applier that sets the look expressions (three.js `VRMLookAtExpressionApplier`).
#[derive(Debug, Clone)]
pub struct ExpressionApplier {
    pub range_map_horizontal_inner: RangeMap,
    pub range_map_horizontal_outer: RangeMap,
    pub range_map_vertical_down: RangeMap,
    pub range_map_vertical_up: RangeMap,
}

impl ExpressionApplier {
    pub fn new(
        range_map_horizontal_inner: RangeMap,
        range_map_horizontal_outer: RangeMap,
        range_map_vertical_down: RangeMap,
        range_map_vertical_up: RangeMap,
    ) -> Self {
        ExpressionApplier {
            range_map_horizontal_inner,
            range_map_horizontal_outer,
            range_map_vertical_down,
            range_map_vertical_up,
        }
    }

    /// Apply the input angle by setting the look expression weights.
    pub fn apply_yaw_pitch(&mut self, expressions: &mut ExpressionManager, yaw: f32, pitch: f32) {
        if pitch < 0.0 {
            let _ = expressions.set_value("lookDown", 0.0);
            let _ = expressions.set_value("lookUp", self.range_map_vertical_up.map(-pitch));
        } else {
            let _ = expressions.set_value("lookUp", 0.0);
            let _ = expressions.set_value("lookDown", self.range_map_vertical_down.map(pitch));
        }

        if yaw < 0.0 {
            let _ = expressions.set_value("lookLeft", 0.0);
            let _ = expressions.set_value("lookRight", self.range_map_horizontal_outer.map(-yaw));
        } else {
            let _ = expressions.set_value("lookRight", 0.0);
            let _ = expressions.set_value("lookLeft", self.range_map_horizontal_outer.map(yaw));
        }
    }
}

/// The look-at applier type (three.js `VRMLookAtApplier`).
#[derive(Debug, Clone)]
pub enum Applier {
    Bone(BoneApplier),
    Expression(ExpressionApplier),
}

impl Applier {
    pub fn apply_yaw_pitch(
        &mut self,
        scene: &mut Scene,
        expressions: Option<&mut ExpressionManager>,
        yaw: f32,
        pitch: f32,
    ) {
        match self {
            Applier::Bone(applier) => applier.apply_yaw_pitch(scene, yaw, pitch),
            Applier::Expression(applier) => {
                if let Some(expressions) = expressions {
                    applier.apply_yaw_pitch(expressions, yaw, pitch);
                }
            }
        }
    }
}

/// Controls eye gaze movements of a VRM (three.js `VRMLookAt`).
#[derive(Debug, Clone)]
pub struct LookAt {
    pub offset_from_head_bone: Vec3,
    pub auto_update: bool,
    /// The world-space position the eyes look toward.
    pub target: Option<Vec3>,
    pub yaw: f32,
    pub pitch: f32,
    pub applier: Applier,
    head_node: Option<usize>,
}

impl LookAt {
    pub fn new(humanoid: &Humanoid, applier: Applier) -> Self {
        LookAt {
            offset_from_head_bone: Vec3::ZERO,
            auto_update: true,
            target: None,
            yaw: 0.0,
            pitch: 0.0,
            applier,
            head_node: humanoid.get_raw_bone_node(HumanBoneName::Head),
        }
    }

    /// Recompute yaw/pitch from `target` and apply them (three.js `VRMLookAt.update`).
    pub fn update(
        &mut self,
        scene: &mut Scene,
        expressions: Option<&mut ExpressionManager>,
        _delta: f32,
    ) {
        if self.auto_update {
            if let Some(head) = self.head_node {
                if let Some(target) = self.target {
                    let origin_world = scene.node_world_position(head) + self.offset_from_head_bone;
                    let delta_vec = target - origin_world;
                    if delta_vec.length_squared() > 0.0 {
                        let inverse_head_world_rotation = scene.node_world_quaternion(head).inverse();
                        let local_vec = inverse_head_world_rotation * delta_vec;

                        let (azimuth, altitude) = calc_azimuth_altitude(local_vec);
                        let mut pitch = RAD2DEG * sanitize_angle(altitude);
                        if pitch.abs() > 80.0 {
                            pitch = 80.0 * pitch.signum();
                        }
                        self.pitch = pitch;
                        self.yaw = RAD2DEG * sanitize_angle(azimuth);
                    }
                }
            }
        }

        self.applier.apply_yaw_pitch(scene, expressions, self.yaw, self.pitch);
    }
}
