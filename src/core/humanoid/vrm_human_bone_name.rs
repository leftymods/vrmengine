use std::collections::HashMap;

pub fn get_human_bone_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("hips".to_string(), "hips".to_string());
    map.insert("spine".to_string(), "spine".to_string());
    map.insert("head".to_string(), "head".to_string());
    map
}
