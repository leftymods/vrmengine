use vrm_engine::{load_from_path, BoneName};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/AvatarSample_A.vrm".to_string());

    let vrm = load_from_path(&path).unwrap_or_else(|e| {
        eprintln!("failed to load {}: {e}", path);
        std::process::exit(1);
    });

    println!("=== {} ===", path);
    println!("VRM spec version: {}", vrm.version);
    println!(
        "name: {}\nauthors: {}\nlicense: {}",
        vrm.meta.name.as_deref().unwrap_or("-"),
        vrm.meta.authors.join(", "),
        vrm.meta.license.as_deref().unwrap_or("-"),
    );

    println!("\n-- humanoid bones --");
    for (bone, node) in vrm.humanoid.iter() {
        let node_name = vrm
            .node(node)
            .map(|n| n.name())
            .filter(|n| !n.is_empty())
            .unwrap_or("?");
        println!("{:>22} -> node {} ({})", bone, node, node_name);
    }

    println!("\n-- expressions --");
    for id in vrm.expressions.ids() {
        let expr = vrm.expressions.get(id).unwrap();
        println!(
            "{:<12} morph_binds={:<3} material_binds={:<2} binary={}",
            id,
            expr.morph_binds.len(),
            expr.material_binds.len(),
            expr.is_binary,
        );
    }

    println!("\n-- look at --");
    match &vrm.look_at {
        Some(look_at) => {
            println!(
                "mode: {:?}, head: {:?}, left eye: {:?}, right eye: {:?}, offset: {}",
                look_at.mode,
                look_at.head_node,
                look_at.left_eye_node,
                look_at.right_eye_node,
                look_at.offset_from_head_bone,
            );
            println!(
                "range inner: {}/{} outer: {}/{} up: {}/{} down: {}/{}",
                look_at.horizontal_inner.input_max_value,
                look_at.horizontal_inner.output_scale,
                look_at.horizontal_outer.input_max_value,
                look_at.horizontal_outer.output_scale,
                look_at.vertical_up.input_max_value,
                look_at.vertical_up.output_scale,
                look_at.vertical_down.input_max_value,
                look_at.vertical_down.output_scale,
            );
        }
        None => println!("none"),
    }

    println!("\n-- spring bones --");
    println!(
        "groups: {}, particles: {}, colliders: {}",
        vrm.spring_bones.group_count(),
        vrm.spring_bones.particle_count(),
        vrm.spring_bones.colliders.len(),
    );
    for (i, group) in vrm.spring_bones.groups.iter().enumerate() {
        println!(
            "  group {}: {:?} particles={} collider_groups={:?} stiffness={} drag={} gravity={} {}",
            i,
            group.name,
            group.particles.len(),
            group.collider_groups,
            group.stiffness,
            group.drag_force,
            group.gravity_power,
            group.gravity_dir,
        );
    }

    println!("\n-- first person --");
    println!(
        "bone: {:?}, offset: {}, annotated nodes: {}, annotated meshes: {}",
        vrm.first_person.bone,
        vrm.first_person.offset,
        vrm.first_person.node_flags.iter().filter(|f| f.is_some()).count(),
        vrm.first_person.mesh_flags.iter().filter(|f| f.is_some()).count(),
    );
    for mesh in vrm.doc.meshes() {
        let visible_fp = vrm.is_mesh_visible(mesh.index(), vrm_engine::FirstPersonCamera::FirstPerson);
        println!(
            "  mesh {} ({:?}) flag={:?} first_person={}",
            mesh.index(),
            mesh.name(),
            vrm.mesh_first_person_flag(mesh.index()),
            visible_fp,
        );
    }

    // sanity: bones should exist for required humanoid parts
    for required in [
        BoneName::Hips,
        BoneName::Spine,
        BoneName::Chest,
        BoneName::Head,
    ] {
        let ok = vrm.human_bone(required).is_some();
        println!("\nrequired bone {:>10}: {}", required, if ok { "OK" } else { "MISSING" });
    }
}
