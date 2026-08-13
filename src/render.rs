

pub fn render_frame() {
    unsafe {
        gl::ClearColor(0.1, 0.1, 0.1, 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT);
    }
}

pub fn run_headless() {
    println!("Headless render mode");
    gl::load_with(|symbol| {
        std::ffi::CString::new(symbol.as_bytes()).unwrap();
        std::ptr::null()
    });
    render_frame();
}
