pub struct VRMMeta {
    pub meta_version: u32,
    pub title: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub contact_information: Option<String>,
    pub reference: Option<String>,
    pub texture_size_limit: Option<u32>,
    pub thumbnail_image: Option<String>,
}

impl VRMMeta {
    pub fn new(meta_version: u32) -> Self {
        Self {
            meta_version,
            title: None,
            version: None,
            author: None,
            contact_information: None,
            reference: None,
            texture_size_limit: None,
            thumbnail_image: None,
        }
    }
}
