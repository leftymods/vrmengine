fn main() {
    vrmengine::viewer::log("VRM Engine started");

    let dummy_data = b"dummy vrm data";
    match vrmengine::model::load_vrm(dummy_data) {
        Ok(_) => vrmengine::viewer::log("Loader OK (dummy)"),
        Err(e) => {
            vrmengine::viewer::log(&format!("Loader error (expected for dummy): {}", e));
        }
    }

    vrmengine::viewer::run();
}
