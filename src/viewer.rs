use std::io::Write;

pub fn log(msg: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!("[{}] {}\n", ts, msg);
    println!("{}", line.trim());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("vrmengine-progress.log")
        .unwrap_or_else(|_| std::fs::File::create("vrmengine-progress.log").unwrap());
    let _ = file.write_all(line.as_bytes());
}

pub fn run() {
    log("Viewer started (winit + glutin)");
    let event_loop = match winit::event_loop::EventLoop::builder().build() {
        Ok(el) => {
            log("Event loop created");
            el
        }
        Err(e) => {
            log(&format!("Event loop creation failed: {}", e));
            std::process::exit(1);
        }
    };

    let attrs = winit::window::WindowAttributes::default()
        .with_title("VRM Engine")
        .with_inner_size(winit::dpi::PhysicalSize::new(800, 600));

    log("Window init started");
    #[allow(deprecated)]
    let _window = match event_loop.create_window(attrs) {
        Ok(w) => {
            log("Window created");
            w
        }
        Err(e) => {
            log(&format!("Window creation failed: {}", e));
            std::process::exit(1);
        }
    };

    log("Viewer init complete");
}
