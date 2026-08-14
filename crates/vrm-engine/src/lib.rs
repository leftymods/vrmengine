//! # vrm-engine
//!
//! A renderer-agnostic runtime engine for [VRM](https://vrm.dev) models, built
//! on top of [`vrm-spec`] (data structures) and [`gltf`] (model loading).
//!
//! The engine loads VRM 0.0 and VRM 1.0 files and provides:
//!
//! - a runtime scene graph with per-node local/world transforms
//! - the humanoid bone mapping ([`BoneName`])
//! - expressions / blend shapes ([`ExpressionManager`])
//! - eye gaze control ([`LookAtController`])
//! - spring bone physics ([`SpringBoneController`])
//! - first-person mesh visibility ([`FirstPerson`])
//!
//! The engine does not render anything; a renderer can consume node world
//! matrices, morph target weights and metadata directly.
//!
//! ## Example
//!
//! ```no_run
//! use vrm_engine::{load_from_path, expression::ExpressionId};
//!
//! let mut vrm = load_from_path("model.vrm")?;
//!
//! // Blink.
//! vrm.set_expression(&ExpressionId::preset("blink").unwrap(), 1.0);
//! vrm.apply_expressions();
//!
//! // Look at a point 2m in front of the head.
//! vrm.update_look_at(glam::Vec3::new(0.0, 1.5, 2.0));
//!
//! // Simulate hair physics for one frame.
//! vrm.update_spring_bones(1.0 / 60.0);
//!
//! vrm.update_transforms();
//! let head = vrm.human_bone(vrm_engine::BoneName::Head).unwrap();
//! let head_matrix = vrm.world_matrix(head);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod bone;
pub mod expression;
pub mod first_person;
pub mod humanoid;
pub mod loader;
pub mod look_at;
pub mod meta;
pub mod spring_bone;
pub mod transform;
pub mod vrm;

pub use bone::BoneName;
pub use expression::{Expression, ExpressionId, ExpressionManager, ExpressionPreset};
pub use first_person::{FirstPerson, FirstPersonCamera, FirstPersonFlag};
pub use humanoid::Humanoid;
pub use loader::{
    load_from_bytes, load_from_path, load_glb_from_bytes, load_glb_from_path, LoadedModel,
    VrmError,
};
pub use look_at::{LookAtController, LookAtMode, RangeMap};
pub use meta::VrmMeta;
pub use spring_bone::{Collider, SpringBoneController, SpringGroup, SpringParticle};
pub use transform::Transform;
pub use vrm::{Node, Vrm, VrmVersion};
