fn main() {
    println!("VRM Engine started");
    // Load a dummy VRM buffer
    let dummy_data = b"dummy vrm data";
    if let Err(e) = vrmengine::model::load_vrm(dummy_data) {
        println!("Loader error (expected for dummy): {}", e);
    }
    vrmengine::viewer::run();
}
