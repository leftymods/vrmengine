//! Unified model metadata extracted from VRM 0.0 and VRM 1.0.

use vrm_spec::vrm_0_0;
use vrm_spec::vrmc_vrm_1_0;

/// Metadata of a VRM model. Not all fields are present in every model or
/// version; all fields are optional and fall back to `None`/empty.
#[derive(Debug, Clone, Default)]
pub struct VrmMeta {
    pub name: Option<String>,
    pub version: Option<String>,
    pub authors: Vec<String>,
    /// License URL (VRM 1.0) or license type + other license URL (VRM 0.0).
    pub license: Option<String>,
    pub contact_information: Option<String>,
    pub references: Vec<String>,
    pub copyright_information: Option<String>,
    pub third_party_licenses: Option<String>,
    /// VRM 1.0: image index. VRM 0.0: texture index.
    pub thumbnail_image: Option<usize>,
    pub allow_redistribution: Option<bool>,
    pub allow_excessively_violent_usage: Option<bool>,
    pub allow_excessively_sexual_usage: Option<bool>,
    pub allow_antisocial_or_hate_usage: Option<bool>,
    pub allow_political_or_religious_usage: Option<bool>,
    pub commercial_usage: Option<String>,
    pub credit_notation: Option<String>,
    pub avatar_permission: Option<String>,
}

fn enum_str<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub(crate) fn load_meta_vrm1(schema: &vrmc_vrm_1_0::VRMCVrmSchema) -> VrmMeta {
    let meta = &schema.meta;
    VrmMeta {
        name: Some(meta.name.clone()),
        version: meta.version.clone(),
        authors: meta.authors.clone(),
        license: Some(meta.license_url.clone()),
        contact_information: meta.contact_information.clone(),
        references: meta.references.clone().unwrap_or_default(),
        copyright_information: meta.copyright_information.clone(),
        third_party_licenses: meta.third_party_licenses.clone(),
        thumbnail_image: meta.thumbnail_image.map(|i| i.value()),
        allow_redistribution: meta.allow_redistribution,
        allow_excessively_violent_usage: meta.allow_excessively_violent_usage,
        allow_excessively_sexual_usage: meta.allow_excessively_sexual_usage,
        allow_antisocial_or_hate_usage: meta.allow_antisocial_or_hate_usage,
        allow_political_or_religious_usage: meta.allow_political_or_religious_usage,
        commercial_usage: meta.commercial_usage.as_ref().and_then(enum_str),
        credit_notation: meta.credit_notation.as_ref().and_then(enum_str),
        avatar_permission: meta.avatar_permission.as_ref().and_then(enum_str),
    }
}

pub(crate) fn load_meta_vrm0(schema: &vrm_0_0::VRM0Schema) -> VrmMeta {
    let Some(meta) = &schema.meta else {
        return VrmMeta::default();
    };
    let mut out = VrmMeta {
        name: meta.title.clone(),
        version: meta.version.clone(),
        authors: meta.author.clone().map(|a| vec![a]).unwrap_or_default(),
        license: meta
            .license_name
            .as_ref()
            .and_then(enum_str)
            .or_else(|| meta.other_license_url.clone()),
        contact_information: meta.contact_information.clone(),
        references: meta.reference.clone().map(|r| vec![r]).unwrap_or_default(),
        thumbnail_image: meta.texture.map(|t| t.value()),
        ..VrmMeta::default()
    };
    if let Some(other) = &meta.other_permission_url {
        if out.contact_information.is_none() {
            out.contact_information = Some(other.clone());
        }
    }
    out
}
