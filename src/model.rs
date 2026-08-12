use gltf::Gltf;
use image::DynamicImage;

pub fn load_vrm(data: &[u8]) -> Result<(), anyhow::Error> {
    println!("Loading VRM model ({} bytes)", data.len());
    let doc = Gltf::from_slice(data)?;
    println!("glTF loaded: {} nodes, {} images", doc.document.nodes().len(), doc.document.images().len());

    for img in doc.document.images() {
        println!("Image: {:?} (source: {:?})", img.index(), img.source());
        if let Some(blob) = &doc.blob {
            if let gltf::image::Source::View { view, .. } = img.source() {
                let offset = view.offset();
                let length = view.length();
                if offset + length <= blob.len() {
                    let image_data = &blob[offset..offset + length];
                    match load_texture(image_data) {
                        Ok((_, w, h)) => println!("  -> Loaded texture: {}x{}", w, h),
                        Err(e) => println!("  -> Texture load failed: {}", e),
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn load_texture(data: &[u8]) -> Result<(DynamicImage, u32, u32), anyhow::Error> {
    let img = image::load_from_memory(data)?;
    let w = img.width(); let h = img.height(); Ok((img, w, h))
}
