//! VRM spring bones, ported from `@pixiv/three-vrm-springbone`.
//!
//! Colliders (sphere / capsule / plane), collider groups and spring bone joints simulated with
//! Verlet integration, plus the `SpringBoneManager` that drives the simulation each frame.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::scene::{Scene, IDENTITY};

/// The shape of a spring bone collider.
#[derive(Debug, Clone, Copy)]
pub enum ColliderShape {
    Sphere {
        offset: Vec3,
        radius: f32,
        inside: bool,
    },
    Capsule {
        offset: Vec3,
        tail: Vec3,
        radius: f32,
        inside: bool,
    },
    Plane {
        offset: Vec3,
        normal: Vec3,
    },
}

impl ColliderShape {
    pub fn offset(&self) -> Option<Vec3> {
        match self {
            ColliderShape::Sphere { offset, .. }
            | ColliderShape::Capsule { offset, .. }
            | ColliderShape::Plane { offset, .. } => Some(*offset),
        }
    }

    /// Apply the shape offset to a world matrix (three.js `updateColliderMatrix`).
    fn collider_matrix(&self, matrix_world: &Mat4) -> Mat4 {
        let mut collider_matrix = *matrix_world;
        if let Some(offset) = self.offset() {
            // glam stores matrices column-major; the translation lives in column 3.
            // R * offset + t == `transform_point3(offset)`.
            let new_translation = matrix_world.transform_point3(offset);
            *collider_matrix.col_mut(3) = Vec4::new(new_translation.x, new_translation.y, new_translation.z, 1.0);
        }
        collider_matrix
    }

    /// Calculate a distance and a direction from the collider to the target object.
    /// It's a hit if the distance is negative. The direction is written into `target`.
    pub fn calculate_collision(
        &self,
        matrix_world: &Mat4,
        object_position: Vec3,
        object_radius: f32,
        target: &mut Vec3,
    ) -> f32 {
        let collider_matrix = self.collider_matrix(matrix_world);
        let head = collider_matrix.col(3).truncate();

        match self {
            ColliderShape::Sphere {
                radius,
                inside,
                ..
            } => {
                let delta = object_position - head;
                let length = delta.length();
                let distance = if *inside {
                    radius - object_radius - length
                } else {
                    length - object_radius - radius
                };

                *target = if distance < 0.0 && length > 1e-8 {
                    let mut dir = delta / length;
                    if *inside {
                        dir = -dir;
                    }
                    dir
                } else {
                    delta
                };
                distance
            }
            ColliderShape::Capsule {
                radius,
                inside,
                tail,
                offset,
            } => {
                // direction from the head to the tail, in world space
                let mut to_tail = collider_matrix.transform_point3(*tail - *offset);
                to_tail -= head;
                let length_sq_capsule = to_tail.length_squared();

                let mut delta = object_position - head;
                let dot = to_tail.dot(delta);

                if dot > 0.0 {
                    if length_sq_capsule <= dot {
                        // near the tail
                        delta -= to_tail;
                    } else {
                        // between both ends
                        delta -= to_tail * (dot / length_sq_capsule);
                    }
                }

                let length = delta.length();
                let distance = if *inside {
                    radius - object_radius - length
                } else {
                    length - object_radius - radius
                };

                *target = if distance < 0.0 && length > 1e-8 {
                    let mut dir = delta / length;
                    if *inside {
                        dir = -dir;
                    }
                    dir
                } else {
                    delta
                };
                distance
            }
            ColliderShape::Plane { normal, .. } => {
                let delta = object_position - head;

                // normal matrix = transpose(inverse) of the 3x3 part; translation is ignored
                let normal_matrix = collider_matrix.inverse().transpose();
                let transformed_normal = normal_matrix.transform_vector3(*normal).normalize_or_zero();

                let distance = delta.dot(transformed_normal) - object_radius;

                *target = transformed_normal;
                distance
            }
        }
    }
}

/// A collider bound to a scene node (three.js `VRMSpringBoneCollider`).
#[derive(Debug, Clone)]
pub struct Collider {
    pub node: usize,
    pub shape: ColliderShape,
}

/// A group of colliders (three.js `VRMSpringBoneColliderGroup`).
#[derive(Debug, Clone, Default)]
pub struct ColliderGroup {
    /// Indices into `SpringBoneManager::colliders`.
    pub colliders: Vec<usize>,
    pub name: Option<String>,
}

/// Settings of a spring bone joint (three.js `VRMSpringBoneJointSettings`).
#[derive(Debug, Clone, Copy)]
pub struct JointSettings {
    pub hit_radius: f32,
    pub stiffness: f32,
    pub gravity_power: f32,
    pub gravity_dir: Vec3,
    pub drag_force: f32,
}

impl Default for JointSettings {
    fn default() -> Self {
        JointSettings {
            hit_radius: 0.0,
            stiffness: 1.0,
            gravity_power: 0.0,
            gravity_dir: Vec3::new(0.0, -1.0, 0.0),
            drag_force: 0.4,
        }
    }
}

/// A single joint of a spring bone (three.js `VRMSpringBoneJoint`).
#[derive(Debug, Clone)]
pub struct Joint {
    pub bone: usize,
    pub child: Option<usize>,
    pub settings: JointSettings,
    /// Indices into `SpringBoneManager::collider_groups`.
    pub collider_groups: Vec<usize>,
    /// Optional node used as the reference space of the simulation.
    pub center: Option<usize>,

    current_tail: Vec3,
    prev_tail: Vec3,
    bone_axis: Vec3,
    world_space_bone_length: f32,
    initial_local_matrix: Mat4,
    initial_local_rotation: Quat,
    initial_local_child_position: Vec3,
}

impl Joint {
    pub fn new(bone: usize, child: Option<usize>, settings: JointSettings, collider_groups: Vec<usize>) -> Self {
        Joint {
            bone,
            child,
            settings,
            collider_groups,
            center: None,
            current_tail: Vec3::ZERO,
            prev_tail: Vec3::ZERO,
            bone_axis: Vec3::ZERO,
            world_space_bone_length: 0.0,
            initial_local_matrix: Mat4::IDENTITY,
            initial_local_rotation: Quat::IDENTITY,
            initial_local_child_position: Vec3::ZERO,
        }
    }

    pub fn set_init_state(&mut self, scene: &Scene) {
        let bone = scene.node(self.bone);

        self.initial_local_matrix = bone.local_matrix;
        self.initial_local_rotation = bone.rotation;

        // Remember the initial position of its local child.
        if let Some(child) = self.child {
            self.initial_local_child_position = scene.node(child).translation;
        } else {
            // VRM 0.x requires a 7cm fixed bone length for the final node in a chain.
            self.initial_local_child_position = bone.translation.normalize_or_zero() * 0.07;
        }

        let bone_world = bone.world_matrix.transform_point3(self.initial_local_child_position);
        self.current_tail = self.world_to_center(scene).transform_point3(bone_world);
        self.prev_tail = self.current_tail;

        self.bone_axis = self.initial_local_child_position.normalize_or_zero();
    }

    pub fn reset(&mut self, scene: &mut Scene) {
        let parent_world = scene
            .node(self.bone)
            .parent
            .map(|p| scene.node(p).world_matrix)
            .unwrap_or(IDENTITY);

        {
            let bone = scene.node_mut(self.bone);
            bone.rotation = self.initial_local_rotation;
            bone.update_matrix();
            bone.world_matrix = parent_world * bone.local_matrix;
        }

        let bone_world = scene
            .node(self.bone)
            .world_matrix
            .transform_point3(self.initial_local_child_position);
        self.current_tail = self.world_to_center(scene).transform_point3(bone_world);
        self.prev_tail = self.current_tail;
    }

    pub fn update(
        &mut self,
        scene: &mut Scene,
        colliders: &[Collider],
        collider_groups: &[ColliderGroup],
        delta: f32,
    ) {
        if delta <= 0.0 {
            return;
        }

        let bone_index = self.bone;
        let bone = scene.node(bone_index);
        let parent_world = bone
            .parent
            .map(|p| scene.node(p).world_matrix)
            .unwrap_or(IDENTITY);
        let bone_world_pos = scene.node_world_position(bone_index);

        // world space bone length
        let child_world_pos = if let Some(child) = self.child {
            scene.node_world_position(child)
        } else {
            scene
                .node(bone_index)
                .world_matrix
                .transform_point3(self.initial_local_child_position)
        };
        self.world_space_bone_length = (bone_world_pos - child_world_pos).length();

        // bone axis in world space (three.js `.transformDirection` = rotate + normalize)
        let local_axis = self
            .initial_local_matrix
            .transform_vector3(self.bone_axis)
            .normalize_or_zero();
        let world_axis = parent_world.transform_vector3(local_axis).normalize_or_zero();

        // Verlet integration: compute the next tail position in center space first
        let mut next_tail =
            self.current_tail + (self.current_tail - self.prev_tail) * (1.0 - self.settings.drag_force);

        // convert to world space, then apply stiffness and gravity in world space
        next_tail = self.center_to_world(scene).transform_point3(next_tail);
        next_tail += world_axis * self.settings.stiffness * delta;
        next_tail += self.settings.gravity_dir * self.settings.gravity_power * delta;

        // normalize bone length
        next_tail = (next_tail - bone_world_pos).normalize_or_zero() * self.world_space_bone_length + bone_world_pos;

        // collision
        self.collision(scene, colliders, collider_groups, &mut next_tail, bone_world_pos);

        // update tails
        self.prev_tail = self.current_tail;
        self.current_tail = self.world_to_center(scene).transform_point3(next_tail);

        // convert the tail direction into a bone rotation
        let world_initial_matrix_inv = (parent_world * self.initial_local_matrix).inverse();
        let dir = world_initial_matrix_inv
            .transform_vector3(next_tail)
            .normalize_or_zero();
        let rotation = self.initial_local_rotation * Quat::from_rotation_arc(self.bone_axis, dir);

        let bone = scene.node_mut(bone_index);
        bone.rotation = rotation;
        bone.update_matrix();
        bone.world_matrix = parent_world * bone.local_matrix;
    }

    fn collision(
        &self,
        scene: &Scene,
        colliders: &[Collider],
        collider_groups: &[ColliderGroup],
        tail: &mut Vec3,
        bone_world_pos: Vec3,
    ) {
        for &group_index in &self.collider_groups {
            let Some(group) = collider_groups.get(group_index) else {
                continue;
            };
            for &collider_index in &group.colliders {
                let Some(collider) = colliders.get(collider_index) else {
                    continue;
                };
                let mut dir = Vec3::ZERO;
                let distance = collider.shape.calculate_collision(
                    &scene.node(collider.node).world_matrix,
                    *tail,
                    self.settings.hit_radius,
                    &mut dir,
                );

                if distance < 0.0 {
                    // hit: push the tail out of the collider
                    *tail += dir * -distance;

                    // normalize bone length
                    *tail = (*tail - bone_world_pos).normalize_or_zero() * self.world_space_bone_length
                        + bone_world_pos;
                }
            }
        }
    }

    fn center_to_world(&self, scene: &Scene) -> Mat4 {
        self.center
            .map(|c| scene.node(c).world_matrix)
            .unwrap_or(IDENTITY)
    }

    fn world_to_center(&self, scene: &Scene) -> Mat4 {
        self.center
            .map(|c| scene.node(c).world_matrix.inverse())
            .unwrap_or(IDENTITY)
    }
}

/// The manager that drives spring bone simulation (three.js `VRMSpringBoneManager`).
#[derive(Debug, Clone, Default)]
pub struct SpringBoneManager {
    pub colliders: Vec<Collider>,
    pub collider_groups: Vec<ColliderGroup>,
    pub joints: Vec<Joint>,
}

impl SpringBoneManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute a joint order such that parent joints are updated before their children.
    fn sorted_joints(&self, scene: &Scene) -> Vec<usize> {
        let mut visited = vec![false; self.joints.len()];
        let mut order = Vec::with_capacity(self.joints.len());

        fn visit(
            manager: &SpringBoneManager,
            scene: &Scene,
            index: usize,
            visited: &mut Vec<bool>,
            order: &mut Vec<usize>,
        ) {
            if visited[index] {
                return;
            }
            visited[index] = true;

            // dependencies: joints whose bone is an ancestor of this bone or of its collider nodes
            for dep in 0..manager.joints.len() {
                if dep == index || visited[dep] {
                    continue;
                }
                let dep_bone = manager.joints[dep].bone;
                let this_bone = manager.joints[index].bone;
                if scene.is_descendant_of(this_bone, dep_bone) {
                    visit(manager, scene, dep, visited, order);
                    continue;
                }
                for &group_index in &manager.joints[index].collider_groups {
                    let Some(group) = manager.collider_groups.get(group_index) else {
                        continue;
                    };
                    for &collider_index in &group.colliders {
                        let Some(collider) = manager.colliders.get(collider_index) else {
                            continue;
                        };
                        if scene.is_descendant_of(collider.node, dep_bone) {
                            visit(manager, scene, dep, visited, order);
                        }
                    }
                }
            }

            order.push(index);
        }

        for i in 0..self.joints.len() {
            visit(self, scene, i, &mut visited, &mut order);
        }
        order
    }

    pub fn set_init_state(&mut self, scene: &mut Scene) {
        scene.update_world_matrices();
        let order = self.sorted_joints(scene);
        for i in order {
            let bone = self.joints[i].bone;
            scene.update_world_matrix(bone, true, false);
            let joint = &mut self.joints[i];
            joint.set_init_state(scene);
        }
    }

    pub fn reset(&mut self, scene: &mut Scene) {
        let order = self.sorted_joints(scene);
        for i in order {
            let joint = &mut self.joints[i];
            joint.reset(scene);
        }
    }

    /// Update the spring bone simulation (three.js `VRMSpringBoneManager.update`).
    pub fn update(&mut self, scene: &mut Scene, delta: f32) {
        // refresh collider world matrices
        for collider in &self.colliders {
            scene.update_world_matrix(collider.node, true, false);
        }

        let order = self.sorted_joints(scene);
        for i in order {
            let bone = self.joints[i].bone;
            scene.update_world_matrix(bone, true, false);
            let joint = &mut self.joints[i];
            joint.update(scene, &self.colliders, &self.collider_groups, delta);
        }
    }
}
