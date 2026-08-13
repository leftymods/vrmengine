fn main() {
    vrmengine::log::init();
    if let Err(e) = vrmengine::viewer::run() {
        vrmengine::log::error(&format!("Fatal error: {e}"));
    }
}
