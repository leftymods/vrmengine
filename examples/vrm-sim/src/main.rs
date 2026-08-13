use glam::Vec3;
use vrm_engine::{load_from_path, BoneName, ExpressionId};

/// Load a model, run a short simulation (expressions + look-at + spring
/// bones) and print the resulting pose.
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/VRM1_Constraint_Twist_Sample.vrm".to_string());

    let mut vrm = load_from_path(&path).unwrap_or_else(|e| {
        eprintln!("failed to load {}: {e}", path);
        std::process::exit(1);
    });

    let head = vrm
        .human_bone(BoneName::Head)
        .unwrap_or_else(|| panic!("no head bone"));
    let left_eye = vrm.human_bone(BoneName::LeftEye);
    let right_eye = vrm.human_bone(BoneName::RightEye);

    let head_start = vrm.world_transform(head).translation;
    println!("head start: {head_start:.3}");

    // Expression: blink + smile.
    if vrm.expressions.contains(&ExpressionId::preset("blink").unwrap()) {
        vrm.set_expression(&ExpressionId::preset("blink").unwrap(), 1.0);
    }
    for id in ["happy", "relaxed"] {
        if let Some(id) = ExpressionId::preset(id) {
            if vrm.expressions.contains(&id) {
                vrm.set_expression(&id, 0.5);
            }
        }
    }
    vrm.apply_expressions();
    for node in 0..vrm.node_count() {
        if let Some(weights) = vrm.morph_weights(node) {
            if weights.iter().any(|&w| w.abs() > 0.001) {
                let node_name = vrm.node(node).unwrap().name();
                println!(
                    "node {node} ({node_name}) morph weights: {:?}",
                    weights.iter().map(|w| (w * 100.0).round() / 100.0).collect::<Vec<_>>()
                );
            }
        }
    }

    // Look at a point in front of and above the head.
    let target = head_start + Vec3::new(0.3, 0.15, 1.0);
    vrm.update_look_at(target);
    vrm.update_transforms();
    println!("look target: {target:.3}");
    if let Some(look_at) = &vrm.look_at {
        let result = look_at.evaluate(target, &vrm.nodes);
        println!(
            "look at yaw={:.1}deg pitch={:.1}deg left_eye={:?} right_eye={:?} expr={:?}",
            result.yaw_deg,
            result.pitch_deg,
            result.left_eye_rotation,
            result.right_eye_rotation,
            result.expression_weights,
        );
    }

    // Spring bone simulation.
    if !vrm.spring_bones.is_empty() {
        for frame in 0..120 {
            vrm.update_spring_bones(1.0 / 60.0);
            if frame == 119 {
                let particles = vrm.spring_bones.particle_count();
                println!("spring bones after 120 frames: {particles} particles");
                for (gi, group) in vrm.spring_bones.groups.iter().enumerate() {
                    if let Some(first) = group.particles.first() {
                        println!(
                            "  group {gi} first particle current={:.3} prev={:.3}",
                            first.current, first.prev
                        );
                    }
                }
            }
        }
    }

    // Final head pose.
    vrm.update_transforms();
    let head_end = vrm.world_transform(head).translation;
    println!("head end: {head_end:.3}");
    if let Some(eye) = left_eye {
        println!("left eye world: {}", vrm.world_transform(eye).translation);
    }
    if let Some(eye) = right_eye {
        println!("right eye world: {}", vrm.world_transform(eye).translation);
    }
}
