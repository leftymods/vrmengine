//! The top-level VRM model, ported from `@pixiv/three-vrm/VRM`.
//!
//! Wraps the scene graph and the VRM components (humanoid, expressions, look-at, first-person,
//! spring bones, meta). `update(delta)` drives the components in the same order as `VRMCore.update`.

use crate::scene::Scene;
use crate::vrm::expression::ExpressionManager;
use crate::vrm::firstperson::FirstPerson;
use crate::vrm::humanoid::Humanoid;
use crate::vrm::lookat::LookAt;
use crate::vrm::meta::VrmMeta;
use crate::vrm::springbone::SpringBoneManager;

#[derive(Debug)]
pub struct VRM {
    pub scene: Scene,
    pub humanoid: Option<Humanoid>,
    pub expressions: Option<ExpressionManager>,
    pub look_at: Option<LookAt>,
    pub first_person: Option<FirstPerson>,
    pub spring_bone_manager: Option<SpringBoneManager>,
    pub meta: Option<VrmMeta>,
}

impl VRM {
    /// Update the model. Call once per frame with the elapsed time in seconds.
    ///
    /// Mirrors `VRMCore.update`: humanoid, expressions, look-at, then spring bones.
    pub fn update(&mut self, delta: f32) {
        if let Some(humanoid) = &self.humanoid {
            humanoid.update(&mut self.scene);
        }

        if let Some(expressions) = &mut self.expressions {
            expressions.update(&mut self.scene);
        }

        if let Some(look_at) = &mut self.look_at {
            let expressions = match &mut self.expressions {
                Some(expressions) => Some(expressions),
                None => None,
            };
            look_at.update(&mut self.scene, expressions, delta);
        }

        if let Some(spring_bone_manager) = &mut self.spring_bone_manager {
            spring_bone_manager.update(&mut self.scene, delta);
        }
    }

    pub fn humanoid(&self) -> Option<&Humanoid> {
        self.humanoid.as_ref()
    }

    pub fn expressions(&self) -> Option<&ExpressionManager> {
        self.expressions.as_ref()
    }

    pub fn expressions_mut(&mut self) -> Option<&mut ExpressionManager> {
        self.expressions.as_mut()
    }

    pub fn look_at(&self) -> Option<&LookAt> {
        self.look_at.as_ref()
    }

    pub fn look_at_mut(&mut self) -> Option<&mut LookAt> {
        self.look_at.as_mut()
    }

    pub fn spring_bone_manager(&self) -> Option<&SpringBoneManager> {
        self.spring_bone_manager.as_ref()
    }

    pub fn meta(&self) -> Option<&VrmMeta> {
        self.meta.as_ref()
    }
}
