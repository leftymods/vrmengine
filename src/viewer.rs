use std::io::Write;

pub fn log(level: &str, msg: &str) {
    println!("[{}] [{}] {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), level, msg);
}
pub fn info(msg: &str) { log("INFO", msg); }
pub fn warn(msg: &str) { log("WARN", msg); }
pub fn error(msg: &str) { log("ERROR", msg); }
pub fn debug(msg: &str) { log("DEBUG", msg); }

pub fn run() {
    info("Viewer started");
    info("Creating event loop");
    let event_loop = winit::event_loop::EventLoop::builder().build().unwrap();
    info("Event loop created");

    info("Creating window");
    let attrs = winit::window::WindowAttributes::default()
        .with_title("VRM Engine")
        .with_inner_size(winit::dpi::PhysicalSize::new(800, 600));
    let window = event_loop.create_window(attrs).unwrap();
    info("Window created");

    info("Initializing GL");
    gl::load_with(|symbol| std::ffi::CString::new(symbol.as_bytes()).unwrap().as_ptr() as _);
    info("GL loaded");

    info("Starting render loop");
    for i in 0..5 {
        debug(&format!("Frame {}", i));
        crate::render::render_frame();
        info(&format!("Rendered frame {}", i));
    }
    info("Render loop complete");
    info("Viewer finished");
}
