//! VRM humanoid bones, ported from `@pixiv/three-vrm-core/humanoid`.
//!
//! Includes the raw rig (`VRMRig`), the normalized rig (`VRMHumanoidRig`) and `VRMHumanoid`
//! with pose get/set/reset semantics.

use std::collections::HashMap;

use glam::{Quat, Vec3};

use crate::scene::Scene;

/// VRM human bone names. Matches `VRMHumanBoneName` in three-vrm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HumanBoneName {
    Hips,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    LeftEye,
    RightEye,
    Jaw,
    LeftUpperLeg,
    LeftLowerLeg,
    LeftFoot,
    LeftToes,
    RightUpperLeg,
    RightLowerLeg,
    RightFoot,
    RightToes,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
    LeftThumbMetacarpal,
    LeftThumbProximal,
    LeftThumbDistal,
    LeftIndexProximal,
    LeftIndexIntermediate,
    LeftIndexDistal,
    LeftMiddleProximal,
    LeftMiddleIntermediate,
    LeftMiddleDistal,
    LeftRingProximal,
    LeftRingIntermediate,
    LeftRingDistal,
    LeftLittleProximal,
    LeftLittleIntermediate,
    LeftLittleDistal,
    RightThumbMetacarpal,
    RightThumbProximal,
    RightThumbDistal,
    RightIndexProximal,
    RightIndexIntermediate,
    RightIndexDistal,
    RightMiddleProximal,
    RightMiddleIntermediate,
    RightMiddleDistal,
    RightRingProximal,
    RightRingIntermediate,
    RightRingDistal,
    RightLittleProximal,
    RightLittleIntermediate,
    RightLittleDistal,
}

impl HumanBoneName {
    pub fn from_str(s: &str) -> Option<Self> {
        use HumanBoneName::*;
        Some(match s {
            "hips" => Hips,
            "spine" => Spine,
            "chest" => Chest,
            "upperChest" => UpperChest,
            "neck" => Neck,
            "head" => Head,
            "leftEye" => LeftEye,
            "rightEye" => RightEye,
            "jaw" => Jaw,
            "leftUpperLeg" => LeftUpperLeg,
            "leftLowerLeg" => LeftLowerLeg,
            "leftFoot" => LeftFoot,
            "leftToes" => LeftToes,
            "rightUpperLeg" => RightUpperLeg,
            "rightLowerLeg" => RightLowerLeg,
            "rightFoot" => RightFoot,
            "rightToes" => RightToes,
            "leftShoulder" => LeftShoulder,
            "leftUpperArm" => LeftUpperArm,
            "leftLowerArm" => LeftLowerArm,
            "leftHand" => LeftHand,
            "rightShoulder" => RightShoulder,
            "rightUpperArm" => RightUpperArm,
            "rightLowerArm" => RightLowerArm,
            "rightHand" => RightHand,
            "leftThumbMetacarpal" => LeftThumbMetacarpal,
            "leftThumbProximal" => LeftThumbProximal,
            "leftThumbDistal" => LeftThumbDistal,
            "leftIndexProximal" => LeftIndexProximal,
            "leftIndexIntermediate" => LeftIndexIntermediate,
            "leftIndexDistal" => LeftIndexDistal,
            "leftMiddleProximal" => LeftMiddleProximal,
            "leftMiddleIntermediate" => LeftMiddleIntermediate,
            "leftMiddleDistal" => LeftMiddleDistal,
            "leftRingProximal" => LeftRingProximal,
            "leftRingIntermediate" => LeftRingIntermediate,
            "leftRingDistal" => LeftRingDistal,
            "leftLittleProximal" => LeftLittleProximal,
            "leftLittleIntermediate" => LeftLittleIntermediate,
            "leftLittleDistal" => LeftLittleDistal,
            "rightThumbMetacarpal" => RightThumbMetacarpal,
            "rightThumbProximal" => RightThumbProximal,
            "rightThumbDistal" => RightThumbDistal,
            "rightIndexProximal" => RightIndexProximal,
            "rightIndexIntermediate" => RightIndexIntermediate,
            "rightIndexDistal" => RightIndexDistal,
            "rightMiddleProximal" => RightMiddleProximal,
            "rightMiddleIntermediate" => RightMiddleIntermediate,
            "rightMiddleDistal" => RightMiddleDistal,
            "rightRingProximal" => RightRingProximal,
            "rightRingIntermediate" => RightRingIntermediate,
            "rightRingDistal" => RightRingDistal,
            "rightLittleProximal" => RightLittleProximal,
            "rightLittleIntermediate" => RightLittleIntermediate,
            "rightLittleDistal" => RightLittleDistal,
            _ => return None,
        })
    }

    /// The dependency-aware list of all human bone names (three.js `VRMHumanBoneList`).
    pub const LIST: &'static [HumanBoneName] = &[
        HumanBoneName::Hips,
        HumanBoneName::Spine,
        HumanBoneName::Chest,
        HumanBoneName::UpperChest,
        HumanBoneName::Neck,
        HumanBoneName::Head,
        HumanBoneName::LeftEye,
        HumanBoneName::RightEye,
        HumanBoneName::Jaw,
        HumanBoneName::LeftUpperLeg,
        HumanBoneName::LeftLowerLeg,
        HumanBoneName::LeftFoot,
        HumanBoneName::LeftToes,
        HumanBoneName::RightUpperLeg,
        HumanBoneName::RightLowerLeg,
        HumanBoneName::RightFoot,
        HumanBoneName::RightToes,
        HumanBoneName::LeftShoulder,
        HumanBoneName::LeftUpperArm,
        HumanBoneName::LeftLowerArm,
        HumanBoneName::LeftHand,
        HumanBoneName::RightShoulder,
        HumanBoneName::RightUpperArm,
        HumanBoneName::RightLowerArm,
        HumanBoneName::RightHand,
        HumanBoneName::LeftThumbMetacarpal,
        HumanBoneName::LeftThumbProximal,
        HumanBoneName::LeftThumbDistal,
        HumanBoneName::LeftIndexProximal,
        HumanBoneName::LeftIndexIntermediate,
        HumanBoneName::LeftIndexDistal,
        HumanBoneName::LeftMiddleProximal,
        HumanBoneName::LeftMiddleIntermediate,
        HumanBoneName::LeftMiddleDistal,
        HumanBoneName::LeftRingProximal,
        HumanBoneName::LeftRingIntermediate,
        HumanBoneName::LeftRingDistal,
        HumanBoneName::LeftLittleProximal,
        HumanBoneName::LeftLittleIntermediate,
        HumanBoneName::LeftLittleDistal,
        HumanBoneName::RightThumbMetacarpal,
        HumanBoneName::RightThumbProximal,
        HumanBoneName::RightThumbDistal,
        HumanBoneName::RightIndexProximal,
        HumanBoneName::RightIndexIntermediate,
        HumanBoneName::RightIndexDistal,
        HumanBoneName::RightMiddleProximal,
        HumanBoneName::RightMiddleIntermediate,
        HumanBoneName::RightMiddleDistal,
        HumanBoneName::RightRingProximal,
        HumanBoneName::RightRingIntermediate,
        HumanBoneName::RightRingDistal,
        HumanBoneName::RightLittleProximal,
        HumanBoneName::RightLittleIntermediate,
        HumanBoneName::RightLittleDistal,
    ];

    /// The parent bone name (three.js `VRMHumanBoneParentMap`).
    pub fn parent(self) -> Option<HumanBoneName> {
        use HumanBoneName::*;
        Some(match self {
            Hips => return None,
            Spine => Hips,
            Chest => Spine,
            UpperChest => Chest,
            Neck => UpperChest,
            Head => Neck,
            LeftEye => Head,
            RightEye => Head,
            Jaw => Head,
            LeftUpperLeg => Hips,
            LeftLowerLeg => LeftUpperLeg,
            LeftFoot => LeftLowerLeg,
            LeftToes => LeftFoot,
            RightUpperLeg => Hips,
            RightLowerLeg => RightUpperLeg,
            RightFoot => RightLowerLeg,
            RightToes => RightFoot,
            LeftShoulder => UpperChest,
            LeftUpperArm => LeftShoulder,
            LeftLowerArm => LeftUpperArm,
            LeftHand => LeftLowerArm,
            RightShoulder => UpperChest,
            RightUpperArm => RightShoulder,
            RightLowerArm => RightUpperArm,
            RightHand => RightLowerArm,
            LeftThumbMetacarpal => LeftHand,
            LeftThumbProximal => LeftThumbMetacarpal,
            LeftThumbDistal => LeftThumbProximal,
            LeftIndexProximal => LeftHand,
            LeftIndexIntermediate => LeftIndexProximal,
            LeftIndexDistal => LeftIndexIntermediate,
            LeftMiddleProximal => LeftHand,
            LeftMiddleIntermediate => LeftMiddleProximal,
            LeftMiddleDistal => LeftMiddleIntermediate,
            LeftRingProximal => LeftHand,
            LeftRingIntermediate => LeftRingProximal,
            LeftRingDistal => LeftRingIntermediate,
            LeftLittleProximal => LeftHand,
            LeftLittleIntermediate => LeftLittleProximal,
            LeftLittleDistal => LeftLittleIntermediate,
            RightThumbMetacarpal => RightHand,
            RightThumbProximal => RightThumbMetacarpal,
            RightThumbDistal => RightThumbProximal,
            RightIndexProximal => RightHand,
            RightIndexIntermediate => RightIndexProximal,
            RightIndexDistal => RightIndexIntermediate,
            RightMiddleProximal => RightHand,
            RightMiddleIntermediate => RightMiddleProximal,
            RightMiddleDistal => RightMiddleIntermediate,
            RightRingProximal => RightHand,
            RightRingIntermediate => RightRingProximal,
            RightRingDistal => RightRingIntermediate,
            RightLittleProximal => RightHand,
            RightLittleIntermediate => RightLittleProximal,
            RightLittleDistal => RightLittleIntermediate,
        })
    }

    /// Names of the required bones (three.js `VRMRequiredHumanBoneName`).
    pub const REQUIRED: &'static [HumanBoneName] = &[
        HumanBoneName::Hips,
        HumanBoneName::Spine,
        HumanBoneName::Head,
        HumanBoneName::LeftUpperLeg,
        HumanBoneName::LeftLowerLeg,
        HumanBoneName::LeftFoot,
        HumanBoneName::RightUpperLeg,
        HumanBoneName::RightLowerLeg,
        HumanBoneName::RightFoot,
        HumanBoneName::LeftUpperArm,
        HumanBoneName::LeftLowerArm,
        HumanBoneName::LeftHand,
        HumanBoneName::RightUpperArm,
        HumanBoneName::RightLowerArm,
        HumanBoneName::RightHand,
    ];
}

/// A pose of a single bone: local position + rotation relative to the rest pose.
#[derive(Debug, Clone, Copy, Default)]
pub struct PoseTransform {
    pub position: Option<Vec3>,
    pub rotation: Option<Quat>,
}

pub type Pose = HashMap<HumanBoneName, PoseTransform>;

/// Map from human bone name to the scene node index it is bound to.
pub type HumanBones = HashMap<HumanBoneName, usize>;

/// The raw rig (`VRMRig` in three-vrm). Bones point into the scene graph.
#[derive(Debug, Clone)]
pub struct Rig {
    pub human_bones: HumanBones,
    pub rest_pose: Pose,
}

impl Rig {
    pub fn new(human_bones: HumanBones, scene: &Scene) -> Self {
        let rest_pose = compute_absolute_pose(&human_bones, scene);
        Rig { human_bones, rest_pose }
    }

    pub fn get_bone_node(&self, name: HumanBoneName) -> Option<usize> {
        self.human_bones.get(&name).copied()
    }

    pub fn get_absolute_pose(&self, scene: &Scene) -> Pose {
        compute_absolute_pose(&self.human_bones, scene)
    }

    pub fn get_pose(&self, scene: &Scene) -> Pose {
        let mut pose = Pose::new();
        for (bone_name, node_index) in &self.human_bones {
            let node = scene.node(*node_index);
            let rest = self.rest_pose.get(bone_name);
            let mut position = node.translation;
            let mut rotation = node.rotation;
            if let Some(rest) = rest {
                if let Some(rest_pos) = rest.position {
                    position -= rest_pos;
                }
                if let Some(rest_rot) = rest.rotation {
                    rotation = rest_rot.inverse() * rotation;
                }
            }
            pose.insert(
                *bone_name,
                PoseTransform {
                    position: Some(position),
                    rotation: Some(rotation),
                },
            );
        }
        pose
    }

    pub fn set_pose(&self, scene: &mut Scene, pose: &Pose) {
        for (bone_name, state) in pose {
            let Some(node_index) = self.human_bones.get(bone_name).copied() else {
                continue;
            };
            let rest = self.rest_pose.get(bone_name).copied();
            let node = scene.node_mut(node_index);
            if let Some(position) = state.position {
                node.translation = position;
                if let Some(rest) = rest {
                    if let Some(rest_pos) = rest.position {
                        node.translation += rest_pos;
                    }
                }
            }
            if let Some(rotation) = state.rotation {
                node.rotation = rotation;
                if let Some(rest) = rest {
                    if let Some(rest_rot) = rest.rotation {
                        node.rotation = node.rotation * rest_rot;
                    }
                }
            }
            node.update_matrix();
        }
    }

    pub fn reset_pose(&self, scene: &mut Scene) {
        for (bone_name, rest) in &self.rest_pose {
            let Some(node_index) = self.human_bones.get(bone_name).copied() else {
                continue;
            };
            let node = scene.node_mut(node_index);
            if let Some(position) = rest.position {
                node.translation = position;
            }
            if let Some(rotation) = rest.rotation {
                node.rotation = rotation;
            }
            node.update_matrix();
        }
    }
}

fn compute_absolute_pose(human_bones: &HumanBones, scene: &Scene) -> Pose {
    let mut pose = Pose::new();
    for (bone_name, node_index) in human_bones {
        let node = scene.node(*node_index);
        pose.insert(
            *bone_name,
            PoseTransform {
                position: Some(node.translation),
                rotation: Some(node.rotation),
            },
        );
    }
    pose
}

/// The normalized rig (`VRMHumanoidRig` in three-vrm).
///
/// It builds a normalized bone hierarchy in world-space positions and copies poses between
/// the normalized rig and the raw rig every update.
#[derive(Debug, Clone)]
pub struct NormalizedRig {
    pub rig: Rig,
    pub root: usize, // synthetic root node index
    pub original: HumanBones,
    pub parent_world_rotations: HashMap<HumanBoneName, Quat>,
    pub bone_rotations: HashMap<HumanBoneName, Quat>,
}

impl NormalizedRig {
    pub fn new(scene: &mut Scene, original: &Rig) -> Self {
        // 1. compute bone world positions / rotations
        let mut bone_world_positions: HashMap<HumanBoneName, Vec3> = HashMap::new();
        let mut parent_world_rotations: HashMap<HumanBoneName, Quat> = HashMap::new();
        let mut bone_rotations: HashMap<HumanBoneName, Quat> = HashMap::new();

        // Make sure the scene is up-to-date
        scene.update_world_matrices();

        for bone_name in HumanBoneName::LIST {
            let Some(node_index) = original.get_bone_node(*bone_name) else {
                continue;
            };
            let node = scene.node(node_index);
            let world = node.world_matrix;
            let (_, _bone_world_rotation, bone_world_position) = world.to_scale_rotation_translation();
            bone_world_positions.insert(*bone_name, bone_world_position);
            bone_rotations.insert(*bone_name, node.rotation);

            let parent_world_rotation = node
                .parent
                .map(|p| scene.node_world_quaternion(p))
                .unwrap_or(Quat::IDENTITY);
            parent_world_rotations.insert(*bone_name, parent_world_rotation);
        }

        // 2. create a synthetic root node
        let root_index = scene.nodes.len();
        let mut root_node = crate::scene::Node::new(root_index);
        root_node.name = "VRMHumanoidRig".to_string();
        root_node.update_matrix();
        scene.nodes.push(root_node);
        scene.root_nodes.push(root_index);

        // 3. build rig hierarchy
        let mut rig_bones: HumanBones = HashMap::new();
        for bone_name in HumanBoneName::LIST {
            let Some(node_index) = original.get_bone_node(*bone_name) else {
                continue;
            };
            let bone_world_position = bone_world_positions[bone_name];

            // find the nearest parent bone in the human bone hierarchy
            let mut current_bone_name = Some(*bone_name);
            let mut parent_bone_world_position: Option<Vec3> = None;
            while let Some(name) = current_bone_name {
                current_bone_name = name.parent();
                if let Some(name) = current_bone_name {
                    if let Some(pos) = bone_world_positions.get(&name) {
                        parent_bone_world_position = Some(*pos);
                        break;
                    }
                }
            }

            let rig_node_index = scene.nodes.len();
            let mut rig_node = crate::scene::Node::new(rig_node_index);
            rig_node.name = format!("Normalized_{}", scene.node(node_index).name);
            rig_node.translation = bone_world_position;
            if let Some(parent_pos) = parent_bone_world_position {
                rig_node.translation -= parent_pos;
            }
            rig_node.update_matrix();
            scene.nodes.push(rig_node);

            let parent_rig_node_index = match current_bone_name {
                Some(name) => rig_bones.get(&name).copied().unwrap_or(root_index),
                None => root_index,
            };
            scene.nodes[rig_node_index].parent = Some(parent_rig_node_index);
            scene.nodes[parent_rig_node_index].children.push(rig_node_index);

            rig_bones.insert(*bone_name, rig_node_index);
        }

        let rig = Rig {
            human_bones: rig_bones.clone(),
            rest_pose: compute_absolute_pose(&rig_bones, scene),
        };

        NormalizedRig {
            rig,
            root: root_index,
            original: original.human_bones.clone(),
            parent_world_rotations,
            bone_rotations,
        }
    }

    /// Copy the pose from the normalized rig into the raw rig (three.js `VRMHumanoidRig.update`).
    pub fn update(&self, scene: &mut Scene) {
        for bone_name in HumanBoneName::LIST {
            let Some(bone_node_index) = self.original.get(bone_name).copied() else {
                continue;
            };
            let Some(rig_bone_node_index) = self.rig.get_bone_node(*bone_name) else {
                continue;
            };
            let parent_world_rotation = self.parent_world_rotations[bone_name];
            let inv_parent_world_rotation = parent_world_rotation.inverse();
            let bone_rotation = self.bone_rotations[bone_name];

            let rig_quat = scene.node(rig_bone_node_index).rotation;
            let new_quat = inv_parent_world_rotation * rig_quat * parent_world_rotation * bone_rotation;

            // three.js: boneNode.quaternion.copy(rig).multiply(parent).premultiply(inv).multiply(rest)
            let bone = scene.node_mut(bone_node_index);
            bone.rotation = new_quat;

            if *bone_name == HumanBoneName::Hips {
                // move the mass center of the VRM
                scene.update_world_matrix(rig_bone_node_index, true, false);
                let bone_world_position = scene.node_world_position(rig_bone_node_index);
                let parent_index = scene.node(bone_node_index).parent;
                if let Some(parent_index) = parent_index {
                    scene.update_world_matrix(parent_index, true, false);
                    let parent_world_matrix = scene.node(parent_index).world_matrix;
                    let local_position = parent_world_matrix.inverse().transform_point3(bone_world_position);
                    let bone = scene.node_mut(bone_node_index);
                    bone.translation = local_position;
                }
            }

            let bone = scene.node_mut(bone_node_index);
            bone.update_matrix();
        }
    }
}

/// The humanoid component of a VRM (`VRMHumanoid` in three-vrm).
#[derive(Debug, Clone)]
pub struct Humanoid {
    pub auto_update_human_bones: bool,
    pub raw_rig: Rig,
    pub normalized_rig: NormalizedRig,
}

impl Humanoid {
    pub fn new(scene: &mut Scene, human_bones: HumanBones, auto_update_human_bones: bool) -> Self {
        let raw_rig = Rig::new(human_bones, scene);
        let normalized_rig = NormalizedRig::new(scene, &raw_rig);
        Humanoid {
            auto_update_human_bones,
            raw_rig,
            normalized_rig,
        }
    }

    pub fn get_raw_bone_node(&self, name: HumanBoneName) -> Option<usize> {
        self.raw_rig.get_bone_node(name)
    }

    pub fn get_normalized_bone_node(&self, name: HumanBoneName) -> Option<usize> {
        self.normalized_rig.rig.get_bone_node(name)
    }

    pub fn raw_rest_pose(&self) -> &Pose {
        &self.raw_rig.rest_pose
    }

    pub fn normalized_rest_pose(&self) -> &Pose {
        &self.normalized_rig.rig.rest_pose
    }

    pub fn get_raw_pose(&self, scene: &Scene) -> Pose {
        self.raw_rig.get_pose(scene)
    }

    pub fn get_normalized_pose(&self, scene: &Scene) -> Pose {
        self.normalized_rig.rig.get_pose(scene)
    }

    pub fn set_raw_pose(&self, scene: &mut Scene, pose: &Pose) {
        self.raw_rig.set_pose(scene, pose);
    }

    pub fn set_normalized_pose(&self, scene: &mut Scene, pose: &Pose) {
        self.normalized_rig.rig.set_pose(scene, pose);
    }

    pub fn reset_raw_pose(&self, scene: &mut Scene) {
        self.raw_rig.reset_pose(scene);
    }

    pub fn reset_normalized_pose(&self, scene: &mut Scene) {
        self.normalized_rig.rig.reset_pose(scene);
    }

    pub fn update(&self, scene: &mut Scene) {
        if self.auto_update_human_bones {
            self.normalized_rig.update(scene);
        }
    }
}
