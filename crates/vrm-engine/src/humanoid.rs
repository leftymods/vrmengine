//! Humanoid bone mapping: bone name -> runtime node index.

use std::collections::HashMap;

use crate::bone::BoneName;

/// Mapping from humanoid bone names to runtime node indices.
#[derive(Debug, Clone, Default)]
pub struct Humanoid {
    bones: HashMap<BoneName, usize>,
}

impl Humanoid {
    /// Create an empty humanoid mapping.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a bone mapping. Returns `true` if it is a new entry.
    pub fn insert(&mut self, bone: BoneName, node: usize) -> bool {
        self.bones.insert(bone, node).is_none()
    }

    /// Get the runtime node index for a humanoid bone.
    pub fn get(&self, bone: BoneName) -> Option<usize> {
        self.bones.get(&bone).copied()
    }

    /// Whether the bone is mapped.
    pub fn contains(&self, bone: BoneName) -> bool {
        self.bones.contains_key(&bone)
    }

    /// Number of mapped bones.
    pub fn count(&self) -> usize {
        self.bones.len()
    }

    /// Iterate over (bone, node) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (BoneName, usize)> + '_ {
        self.bones.iter().map(|(&b, &n)| (b, n))
    }

    /// The hips bone, which is the required root of a VRM humanoid.
    pub fn hips(&self) -> Option<usize> {
        self.get(BoneName::Hips)
    }

    /// The head bone.
    pub fn head(&self) -> Option<usize> {
        self.get(BoneName::Head)
    }

    /// All bone names present in the model.
    pub fn bones(&self) -> Vec<BoneName> {
        self.bones.keys().copied().collect()
    }
}
