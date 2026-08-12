fn main() {
    vrmengine::viewer::info("VRM Engine started");
    vrmengine::viewer::debug("Initializing loader...");

    let dummy_data = b"dummy vrm data";
    match vrmengine::model::load_vrm(dummy_data) {
        Ok(_) => vrmengine::viewer::info("Loader OK (dummy)"),
        Err(e) => {
            vrmengine::viewer::warn(&format!("Loader error (expected for dummy): {}", e));
        }
    }

    vrmengine::viewer::info("Starting viewer...");
    vrmengine::viewer::run();
}
