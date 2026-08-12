use std::io::{Read, Cursor};

pub fn load_vrm(data: &[u8]) -> Result<(), anyhow::Error> {
    // Placeholder VRM loader
    println!("Loading VRM model ({} bytes)", data.len());
    Ok(())
}
