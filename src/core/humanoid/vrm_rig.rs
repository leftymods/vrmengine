use std::collections::HashMap;
use glam::{Vec3, Quat};
use super::vrm_pose::{VRMPose, VRMPoseTransform};

#[derive(Clone, Debug)]
pub struct VRMRig {
    pub human_bones: HashMap<String, VRMHumanBone>,
    pub rest_pose: VRMPose,
}

#[derive(Clone, Debug)]
pub struct VRMHumanBone {
    pub name: String,
    pub node: Object3D,
}

#[derive(Clone, Debug)]
pub struct Object3D {
    pub position: Vec3,
    pub quaternion: Quat,
}

impl VRMRig {
    pub fn new(bones: HashMap<String, VRMHumanBone>) -> Self {
        let mut rig = Self { human_bones: bones, rest_pose: VRMPose::default() };
        rig.rest_pose = rig.get_absolute_pose();
        rig
    }

    pub fn get_absolute_pose(&self) -> VRMPose {
        let mut pose = VRMPose::default();
        for (name, bone) in &self.human_bones {
            pose.transforms.insert(name.clone(), VRMPoseTransform {
                position: Some([bone.node.position.x, bone.node.position.y, bone.node.position.z]),
                rotation: Some([bone.node.quaternion.x, bone.node.quaternion.y, bone.node.quaternion.z, bone.node.quaternion.w]),
            });
        }
        pose
    }

    pub fn get_pose(&self) -> VRMPose {
        self.get_absolute_pose()
    }

    pub fn set_pose(&mut self, pose: &VRMPose) {
        for (name, state) in &pose.transforms {
            if let Some(bone) = self.human_bones.get_mut(name) {
                if let Some(position) = state.position {
                    bone.node.position = Vec3::from_array(position);
                }
                if let Some(rotation) = state.rotation {
                    bone.node.quaternion = Quat::from_array(rotation);
                }
            }
        }
    }

    pub fn reset_pose(&mut self) {
        let rest = self.rest_pose.clone();
        self.set_pose(&rest);
    }

    pub fn get_bone(&self, name: &str) -> Option<VRMHumanBone> {
        self.human_bones.get(name).cloned()
    }

    pub fn get_bone_node(&self, name: &str) -> Option<Object3D> {
        self.human_bones.get(name).map(|b| b.node.clone())
    }
}
