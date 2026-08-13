//! Spring bone physics (secondary animation) for VRM 0.0 and VRM 1.0.
//!
//! This is a Verlet integration based simulation. Each particle is attached to
//! a bone node; gravity, stiffness (returning to the rest pose), air drag and
//! collider spheres/capsules are simulated. The resulting world direction is
//! converted back into a local rotation of the bone node.

use std::collections::HashMap;

use glam::{Quat, Vec3};

use crate::vrm::Node;

/// A spherical or capsule-shaped collider used by spring bone groups.
#[derive(Debug, Clone, Copy)]
pub struct Collider {
    /// Runtime node index the collider is attached to.
    pub node: usize,
    /// Offset from the node in the node's local space.
    pub offset: Vec3,
    /// For capsule colliders, the offset of the capsule tail.
    pub tail: Option<Vec3>,
    pub radius: f32,
    /// Index of the collider group this collider belongs to.
    pub group: usize,
}

/// A single simulated point on a spring bone chain.
#[derive(Debug, Clone)]
pub struct SpringParticle {
    /// Runtime node index of the bone.
    pub node: usize,
    /// The node used as the "parent" anchor for verlet integration. The first
    /// particle of a chain uses the spring's center node (or scene parent).
    pub verlet_parent: Option<usize>,
    /// Scene-graph parent, used to frame the resulting local rotation.
    pub scene_parent: Option<usize>,
    /// Rest direction from the verlet parent to the bone, in the parent's local
    /// space.
    pub local_offset: Vec3,
    /// Rest length between the verlet parent and the bone.
    pub rest_len: f32,
    /// Rest direction from the verlet parent to the bone in world space.
    pub rest_world_dir: Vec3,
    /// World rotation of the bone in the rest pose.
    pub initial_world_rot: Quat,
    /// Previous world position.
    pub prev: Vec3,
    /// Current world position.
    pub current: Vec3,
}

/// A group of spring bone particles.
#[derive(Debug, Clone)]
pub struct SpringGroup {
    pub name: Option<String>,
    /// Center node used as the anchor of the chain.
    pub center: Option<usize>,
    /// Collider groups this spring reacts to.
    pub collider_groups: Vec<usize>,
    pub stiffness: f32,
    pub drag_force: f32,
    pub gravity_dir: Vec3,
    pub gravity_power: f32,
    pub hit_radius: f32,
    pub particles: Vec<SpringParticle>,
}

/// All spring bone groups and colliders of a model.
#[derive(Debug, Clone, Default)]
pub struct SpringBoneController {
    pub groups: Vec<SpringGroup>,
    pub colliders: Vec<Collider>,
}

impl SpringBoneController {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub fn particle_count(&self) -> usize {
        self.groups.iter().map(|g| g.particles.len()).sum()
    }

    /// Reset the simulation state to the rest pose.
    pub fn reset(&mut self, nodes: &[Node]) {
        for group in &mut self.groups {
            for particle in &mut group.particles {
                if let Some(node) = nodes.get(particle.node) {
                    particle.prev = node.world.translation;
                    particle.current = node.world.translation;
                }
            }
        }
    }

    /// Advance the simulation by `dt` seconds.
    ///
    /// Node world transforms must be up to date before calling this.
    pub fn update(&mut self, nodes: &mut [Node], dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        let dt2 = dt * dt;

        // Resolve colliders into world space.
        let world_colliders: Vec<(Vec3, Option<Vec3>, f32)> = self
            .colliders
            .iter()
            .filter_map(|c| {
                let node = nodes.get(c.node)?;
                let center = node.world.translation + node.world.rotation * c.offset;
                let tail = c.tail.map(|t| node.world.translation + node.world.rotation * t);
                Some((center, tail, c.radius))
            })
            .collect();
        let mut group_colliders: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, c) in self.colliders.iter().enumerate() {
            group_colliders.entry(c.group).or_default().push(i);
        }

        for group in &mut self.groups {
            // Colliders relevant to this group.
            let colliders: Vec<(Vec3, Option<Vec3>, f32)> = group
                .collider_groups
                .iter()
                .filter_map(|g| group_colliders.get(g))
                .flatten()
                .filter_map(|&ci| world_colliders.get(ci).copied())
                .collect();

            for particle in &mut group.particles {
                let (parent_pos, parent_rot) = match particle.verlet_parent {
                    Some(p) => match nodes.get(p) {
                        Some(parent) => (parent.world.translation, parent.world.rotation),
                        None => (Vec3::ZERO, Quat::IDENTITY),
                    },
                    None => (Vec3::ZERO, Quat::IDENTITY),
                };

                let current = nodes
                    .get(particle.node)
                    .map(|n| n.world.translation)
                    .unwrap_or(Vec3::ZERO);

                // Verlet integration.
                let velocity = (current - particle.prev) * (1.0 - group.drag_force.clamp(0.0, 1.0));
                let target = parent_pos + parent_rot * particle.local_offset;
                let mut next = current + velocity;
                next += group.gravity_dir * (group.gravity_power * dt2);
                next += (target - current) * (group.stiffness * dt2);

                // Collision resolution.
                for (center, tail, radius) in &colliders {
                    next = resolve_sphere(next, *center, *tail, *radius + group.hit_radius);
                }

                // Length constraint: keep the distance to the parent.
                let dir = next - parent_pos;
                if dir.length_squared() > 1e-9 {
                    next = parent_pos + dir.normalize() * particle.rest_len;
                } else {
                    next = parent_pos + particle.rest_world_dir * particle.rest_len;
                }

                particle.prev = current;
                particle.current = next;

                // Convert the target direction back into a node rotation.
                let desired_dir = if (next - parent_pos).length_squared() > 1e-9 {
                    (next - parent_pos).normalize()
                } else {
                    particle.rest_world_dir
                };
                let delta = Quat::from_rotation_arc(particle.rest_world_dir, desired_dir);
                let world_rot = particle.initial_world_rot * delta;
                let scene_parent_rot = particle
                    .scene_parent
                    .and_then(|p| nodes.get(p))
                    .map(|p| p.world.rotation)
                    .unwrap_or(Quat::IDENTITY);
                if let Some(node) = nodes.get_mut(particle.node) {
                    node.local.rotation = scene_parent_rot.inverse() * world_rot;
                }
            }
        }
    }
}

fn resolve_sphere(pos: Vec3, center: Vec3, tail: Option<Vec3>, min_dist: f32) -> Vec3 {
    let closest = match tail {
        Some(tail) => closest_point_on_segment(pos, center, tail),
        None => center,
    };
    let to = pos - closest;
    let d2 = to.length_squared();
    if d2 < 1e-9 {
        closest + Vec3::X * min_dist
    } else if d2 < min_dist * min_dist {
        closest + to * (min_dist / d2.sqrt())
    } else {
        pos
    }
}

fn closest_point_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let denom = ab.length_squared();
    if denom < 1e-9 {
        return a;
    }
    let t = ((p - a).dot(ab) / denom).clamp(0.0, 1.0);
    a + ab * t
}
