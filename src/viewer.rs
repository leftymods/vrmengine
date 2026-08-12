use std::panic::catch_unwind;

pub fn run() {
    println!("Viewer started (winit + glutin)");
    std::fs::write("vrmengine-progress.log", "viewer run\n").ok();
    let _ = catch_unwind(|| {
        // frame rendering loop
    });
}
