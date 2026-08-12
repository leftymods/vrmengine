use std::collections::HashMap;
use super::vrm_rig::{VRMRig, VRMHumanBone, Object3D};

pub struct VRMHumanoid {
    pub auto_update_human_bones: bool,
    pub raw_human_bones: VRMRig,
    pub normalized_human_bones: VRMRig,
}

impl VRMHumanoid {
    pub fn new(bones: HashMap<String, VRMHumanBone>, auto_update: bool) -> Self {
        let raw = VRMRig::new(bones.clone());
        Self {
            auto_update_human_bones: auto_update,
            raw_human_bones: raw.clone(),
            normalized_human_bones: raw,
        }
    }

    pub fn get_normalized_bone(&self, name: &str) -> Option<VRMHumanBone> {
        self.normalized_human_bones.get_bone(name)
    }

    pub fn get_normalized_bone_node(&self, name: &str) -> Option<Object3D> {
        self.get_normalized_bone(name).map(|b| b.node)
    }

    pub fn update(&mut self) {
        if self.auto_update_human_bones {
            // Transfer pose from normalized to raw in a real implementation
        }
    }
}
