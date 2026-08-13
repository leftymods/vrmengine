//! Small math helpers used across the engine.
//!
//! The engine is renderer-agnostic and only needs the `glam` math types.

use glam::{Mat4, Quat, Vec3};

/// Local or world transform (translation, rotation, scale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

        /// Build a transform from a glTF node transform.
    pub fn from_gltf(t: gltf::scene::Transform) -> Self {
        let (translation, rotation, scale) = t.decomposed();        Self {
            translation: Vec3::from(translation),
            rotation: Quat::from_array(rotation),
            scale: Vec3::from(scale),
        }
    }

    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Decompose a matrix back into a transform.
    ///
    /// Note: shear is not preserved.
    pub fn from_mat4(m: &Mat4) -> Self {
        let (scale, rotation, translation) = m.to_scale_rotation_translation();
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Compose two transforms (`self` first, i.e. parent * child).
    pub fn mul(&self, other: &Transform) -> Transform {
        Transform::from_mat4(&(self.to_mat4() * other.to_mat4()))
    }

    /// Forward direction (local +Z).
    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::Z
    }

    /// Right direction (local +X).
    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    /// Up direction (local +Y).
    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }
}
