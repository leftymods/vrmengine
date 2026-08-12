use std::collections::HashMap;
use glam::{Vec3, Quat};

pub const HUMAN_BONES: &[&str] = &[
    "hips", "spine", "chest", "upperChest", "neck",
    "head", "leftEye", "rightEye", "jaw",
    "leftUpperLeg", "leftLowerLeg", "leftFoot", "leftToes",
    "rightUpperLeg", "rightLowerLeg", "rightFoot", "rightToes",
    "leftShoulder", "leftUpperArm", "leftLowerArm", "leftHand",
    "rightShoulder", "rightUpperArm", "rightLowerArm", "rightHand",
    // thumbs and fingers omitted for brevity
];

#[derive(Clone, Debug)]
pub struct HumanBone {
    pub name: String,
    pub node: Option<Object3D>,
}

#[derive(Clone, Debug)]
pub struct Pose {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub struct VRMRig {
    pub bones: HashMap<String, HumanBone>,
    pub rest_pose: HashMap<String, Pose>,
}

#[derive(Clone, Debug)]
pub struct VRMHumanoid {
    pub raw_bones: VRMRig,
    pub normalized_bones: VRMRig,
    pub auto_update: bool,
}

impl VRMHumanoid {
    pub fn new(bones: HashMap<String, HumanBone>) -> Self {
        let raw = VRMRig { bones: bones.clone(), rest_pose: HashMap::new() };
        Self { raw_bones: raw.clone(), normalized_bones: raw, auto_update: true }
    }

    pub fn get_normalized_bone_node(&self, bone_name: &str) -> Option<Object3D> {
        self.normalized_bones.bones.get(bone_name)?.node.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Object3D {
    pub position: Vec3,
    pub quaternion: Quat,
}
