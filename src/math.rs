//! Small math helpers ported from `@pixiv/three-vrm-core` utilities.

use glam::{Quat, Vec3};

pub const DEG2RAD: f32 = std::f32::consts::PI / 180.0;
pub const RAD2DEG: f32 = 180.0 / std::f32::consts::PI;

/// Clamp the given value into the [0.0, 1.0] range.
pub fn saturate(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// Make sure the angle is within -PI to PI.
pub fn sanitize_angle(angle: f32) -> f32 {
    let round_turn = (angle / 2.0 / std::f32::consts::PI).round();
    angle - 2.0 * std::f32::consts::PI * round_turn
}

/// Calculate azimuth / altitude angles from a vector.
///
/// Azimuth represents an angle around Y axis, altitude an angle around Z axis.
/// Port of `calcAzimuthAltitude`.
pub fn calc_azimuth_altitude(vector: Vec3) -> (f32, f32) {
    let azimuth = (-vector.z).atan2(vector.x);
    let altitude = vector
        .y
        .atan2((vector.x * vector.x + vector.z * vector.z).sqrt());
    (azimuth, altitude)
}

/// Inverse of a unit quaternion, mirroring three.js `Quaternion.invert()`.
pub fn quat_invert_compat(q: Quat) -> Quat {
    q.inverse()
}

/// Extract the world quaternion from a world matrix (three.js `getWorldQuaternionLite`).
pub fn world_matrix_to_quat(world: glam::Mat4) -> Quat {
    let (_, quat, _) = world.to_scale_rotation_translation();
    quat
}

/// Decompose a matrix into a position (three.js `Matrix4.decompose` position part).
pub fn world_matrix_to_position(world: glam::Mat4) -> Vec3 {
    world.to_scale_rotation_translation().2
}
