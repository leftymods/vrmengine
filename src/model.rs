use gltf::Gltf;

pub fn load_vrm(data: &[u8]) -> Result<(), anyhow::Error> {
    println!("Loading VRM model ({} bytes)", data.len());
    // Try to parse as glTF binary
    let _cursor = std::io::Cursor::new(data);
    let doc = Gltf::from_slice(data)?;
    println!("glTF loaded: {} nodes", doc.nodes().len());
    Ok(())
}
