//! First-person mesh visibility handling.

use glam::Vec3;

/// First person mesh annotation flag (VRM 0.0) / mesh annotation type
/// (VRM 1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstPersonFlag {
    /// VRM 1.0: the mesh is automatically determined. VRM 0.0: no annotation
    /// was present.
    Auto,
    Both,
    FirstPersonOnly,
    ThirdPersonOnly,
}

impl FirstPersonFlag {
    pub(crate) fn from_vrm1(kind: vrm_spec::vrmc_vrm_1_0::FirstPersonType) -> Self {
        use vrm_spec::vrmc_vrm_1_0::FirstPersonType as T;
        match kind {
            T::Auto => FirstPersonFlag::Auto,
            T::Both => FirstPersonFlag::Both,
            T::FirstPersonOnly => FirstPersonFlag::FirstPersonOnly,
            T::ThirdPersonOnly => FirstPersonFlag::ThirdPersonOnly,
        }
    }

    pub(crate) fn from_vrm0(flag: &str) -> Self {
        match flag {
            "FirstPersonOnly" => FirstPersonFlag::FirstPersonOnly,
            "ThirdPersonOnly" => FirstPersonFlag::ThirdPersonOnly,
            "Both" => FirstPersonFlag::Both,
            _ => FirstPersonFlag::Auto,
        }
    }
}

/// The camera rendering the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstPersonCamera {
    FirstPerson,
    ThirdPerson,
}

/// First person configuration.
#[derive(Debug, Clone)]
pub struct FirstPerson {
    /// The first person bone (usually the head). Meshes attached to its
    /// subtree are hidden in first person when their flag is `Auto`.
    pub bone: Option<usize>,
    /// Offset of the camera/eye from the first person bone.
    pub offset: Vec3,
    /// Per-node annotations (VRM 1.0).
    pub node_flags: Vec<Option<FirstPersonFlag>>,
    /// Per-mesh annotations (VRM 0.0).
    pub mesh_flags: Vec<Option<FirstPersonFlag>>,
}

impl FirstPerson {
    pub fn empty(node_count: usize, mesh_count: usize) -> Self {
        Self {
            bone: None,
            offset: Vec3::ZERO,
            node_flags: vec![None; node_count],
            mesh_flags: vec![None; mesh_count],
        }
    }
}
