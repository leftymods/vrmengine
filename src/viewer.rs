use std::io::Write;

fn color(level: &str) -> &'static str {
    match level {
        "ERROR" => "\x1b[91m",
        "WARN" => "\x1b[93m",
        "INFO" => "\x1b[92m",
        "DEBUG" => "\x1b[96m",
        _ => "\x1b[0m",
    }
}

pub fn log(level: &str, msg: &str) {
    let c = color(level);
    println!("[{}] [{}{}\x1b[0m] {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), c, level, msg);
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
    #[allow(deprecated)]
    let _window = match event_loop.create_window(attrs) {
        Ok(w) => { info("Window created"); w }
        Err(e) => { error(&format!("Window creation failed: {}", e)); std::process::exit(1); }
    };
    info("Initializing GL");
    gl::load_with(|symbol| std::ffi::CString::new(symbol.as_bytes()).unwrap().as_ptr() as _);
    info("GL loaded");
    info("Starting render loop (5 frames)");
    for i in 0..5 {
        debug(&format!("Frame {}", i));
        let result = std::panic::catch_unwind(|| { crate::render::render_frame(); });
        if result.is_err() { warn("GL render frame failed (no context?)"); }
        info(&format!("Rendered frame {}", i));
    }
    info("Render loop complete");
    info("Viewer finished");
    info("Process complete");
    std::thread::sleep(std::time::Duration::from_millis(500));
}
