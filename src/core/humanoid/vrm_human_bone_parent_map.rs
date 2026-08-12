use std::collections::HashMap;

pub fn get_human_bone_parent_map() -> HashMap<String, Option<String>> {
    let mut map = HashMap::new();
    map.insert("spine".to_string(), Some("hips".to_string()));
    map.insert("head".to_string(), Some("spine".to_string()));
    map
}
