use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let window = args.contains(&"--window".to_string()) || !args.contains(&"--headless".to_string());
    println!("Starting VRM engine, window mode: {}", window);
    // install_crash_handler, log_memory, viewer::run
}
