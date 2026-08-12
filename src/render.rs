use std::thread;

pub fn render_frame() {
    println!("Desktop shader render frame");
}

pub fn run_headless() {
    println!("Headless render mode");
    render_frame();
}
