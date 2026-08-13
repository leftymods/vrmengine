//! Eye gaze ("look at") controller.

use glam::{Quat, Vec3};

use crate::expression::ExpressionPreset;
use crate::vrm::Node;

/// How the look-at controller moves the eyes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookAtMode {
    /// Rotates the eye bones directly.
    Bone,
    /// Drives the `LookUp`/`LookDown`/`LookLeft`/`LookRight` expressions.
    Expression,
}

/// A single angle range mapping.
#[derive(Debug, Clone, Copy)]
pub struct RangeMap {
    /// Input angle clamp in degrees.
    pub input_max_value: f32,
    /// Output scale: degrees of eye rotation (bone mode) or expression weight
    /// (expression mode).
    pub output_scale: f32,
}

impl Default for RangeMap {
    fn default() -> Self {
        Self {
            input_max_value: 10.0,
            output_scale: 1.0,
        }
    }
}

impl RangeMap {
    fn map(&self, angle_deg: f32) -> f32 {
        let max = if self.input_max_value > 0.0 {
            self.input_max_value
        } else {
            1.0
        };
        let normalized = (angle_deg / max).clamp(-1.0, 1.0);
        normalized * self.output_scale
    }
}

/// Eye gaze configuration.
#[derive(Debug, Clone)]
pub struct LookAtController {
    pub mode: LookAtMode,
    /// Runtime node index of the head bone.
    pub head_node: Option<usize>,
    pub left_eye_node: Option<usize>,
    pub right_eye_node: Option<usize>,
    /// Eye position offset from the head bone.
    pub offset_from_head_bone: Vec3,
    pub horizontal_inner: RangeMap,
    pub horizontal_outer: RangeMap,
    pub vertical_up: RangeMap,
    pub vertical_down: RangeMap,
}

/// Result of evaluating the look-at controller for a target.
#[derive(Debug, Clone)]
pub struct LookAtResult {
    /// Yaw in degrees relative to the head forward direction. Positive means the
    /// target is to the model's right (+X).
    pub yaw_deg: f32,
    /// Pitch in degrees. Positive means the target is above the eye line.
    pub pitch_deg: f32,
    /// Eye bone rotations (bone mode).
    pub left_eye_rotation: Option<Quat>,
    pub right_eye_rotation: Option<Quat>,
    /// Expression weights (expression mode).
    pub expression_weights: Vec<(ExpressionPreset, f32)>,
}

impl Default for LookAtController {
    fn default() -> Self {
        Self {
            mode: LookAtMode::Expression,
            head_node: None,
            left_eye_node: None,
            right_eye_node: None,
            offset_from_head_bone: Vec3::ZERO,
            horizontal_inner: RangeMap::default(),
            horizontal_outer: RangeMap::default(),
            vertical_up: RangeMap::default(),
            vertical_down: RangeMap::default(),
        }
    }
}

impl LookAtController {
    /// Compute the eye movement for a world-space target.
    pub fn evaluate(&self, target: Vec3, nodes: &[Node]) -> LookAtResult {
        let mut result = LookAtResult {
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            left_eye_rotation: None,
            right_eye_rotation: None,
            expression_weights: Vec::new(),
        };

        let Some(head_node) = self.head_node else {
            return result;
        };
        let Some(head) = nodes.get(head_node) else {
            return result;
        };

        let head_pos = head.world.translation;
        let head_rot = head.world.rotation;
        let eye_origin = head_pos + head_rot * self.offset_from_head_bone;

        let dir = target - eye_origin;
        if dir.length_squared() < 1e-9 {
            return result;
        }
        let local = head_rot.inverse() * dir.normalize();

        // Forward is +Z in the head's local space.
        let yaw_deg = local.x.atan2(local.z).to_degrees();
        let pitch_deg = local.y.atan2(local.z).to_degrees();
        result.yaw_deg = yaw_deg;
        result.pitch_deg = pitch_deg;

        match self.mode {
            LookAtMode::Bone => {
                let horizontal = |angle_deg: f32, map: RangeMap| deg2rad(map.map(angle_deg));
                // When the target is on the left, the left eye looks further
                // outward (outer map) and the right eye converges inward
                // (inner map); the opposite holds on the right side.
                let (left_yaw, right_yaw) = if yaw_deg < 0.0 {
                    (horizontal(yaw_deg, self.horizontal_outer), horizontal(yaw_deg, self.horizontal_inner))
                } else {
                    (horizontal(yaw_deg, self.horizontal_inner), horizontal(yaw_deg, self.horizontal_outer))
                };
                let (pitch_rad, pitch_map) = if pitch_deg >= 0.0 {
                    (deg2rad(self.vertical_up.map(pitch_deg)), self.vertical_up)
                } else {
                    (deg2rad(self.vertical_down.map(pitch_deg)), self.vertical_down)
                };
                let _ = pitch_map;
                // q = Ry(yaw) * Rx(-pitch) rotates the forward (+Z) gaze toward
                // the target in a right-handed coordinate system.
                let left = Quat::from_rotation_y(left_yaw) * Quat::from_rotation_x(-pitch_rad);
                let right = Quat::from_rotation_y(right_yaw) * Quat::from_rotation_x(-pitch_rad);
                result.left_eye_rotation = Some(left);
                result.right_eye_rotation = Some(right);
            }
            LookAtMode::Expression => {
                let mut add = |preset: ExpressionPreset, weight: f32| {
                    if weight > 0.0 {
                        result.expression_weights.push((preset, weight));
                    }
                };
                let outer = self.horizontal_outer;
                if yaw_deg < 0.0 {
                    add(ExpressionPreset::LookLeft, clamp_weight((-yaw_deg / outer.input_max_value) * outer.output_scale));
                } else {
                    add(ExpressionPreset::LookRight, clamp_weight((yaw_deg / outer.input_max_value) * outer.output_scale));
                }
                if pitch_deg < 0.0 {
                    let down = self.vertical_down;
                    add(ExpressionPreset::LookDown, clamp_weight((-pitch_deg / down.input_max_value) * down.output_scale));
                } else {
                    let up = self.vertical_up;
                    add(ExpressionPreset::LookUp, clamp_weight((pitch_deg / up.input_max_value) * up.output_scale));
                }
            }
        }

        result
    }
}

fn clamp_weight(w: f32) -> f32 {
    w.clamp(0.0, 1.0)
}

fn deg2rad(d: f32) -> f32 {
    d * std::f32::consts::PI / 180.0
}
