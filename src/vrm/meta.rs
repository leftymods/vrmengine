//! VRM metadata, ported from `@pixiv/three-vrm-core/meta`.
//!
//! VRM 1.0 (`VRMC_vrm.meta`) and VRM 0.x (`VRM.meta`) both map to a single `VrmMeta` enum.

/// Enum indicates a condition who can perform with this avatar (VRM 0.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V0AllowedUserName {
    Everyone,
    ExplicitlyLicensedPerson,
    OnlyAuthor,
}

impl V0AllowedUserName {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Everyone" => V0AllowedUserName::Everyone,
            "ExplicitlyLicensedPerson" => V0AllowedUserName::ExplicitlyLicensedPerson,
            "OnlyAuthor" => V0AllowedUserName::OnlyAuthor,
            _ => return None,
        })
    }
}

/// Enum indicates allow or disallow (VRM 0.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V0UsagePermission {
    Allow,
    Disallow,
}

impl V0UsagePermission {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Allow" => Some(V0UsagePermission::Allow),
            "Disallow" => Some(V0UsagePermission::Disallow),
            _ => None,
        }
    }
}

/// Enum indicates allow or disallow commercial use (VRM 0.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V0CommercialUsagePermission {
    Allow,
    AllowWithCredit,
    Disallow,
}

impl V0CommercialUsagePermission {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Allow" => V0CommercialUsagePermission::Allow,
            "AllowWithCredit" => V0CommercialUsagePermission::AllowWithCredit,
            "Disallow" => V0CommercialUsagePermission::Disallow,
            _ => return None,
        })
    }
}

/// Enum indicates a license type (VRM 0.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V0LicenseName {
    RedistributionProhibited,
    CC0,
    CCBy,
    CCByNc,
    CCBySa,
    CCByNcSa,
    CCByNd,
    CCByNcNd,
    Other,
}

impl V0LicenseName {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Redistribution_Prohibited" => V0LicenseName::RedistributionProhibited,
            "CC0" => V0LicenseName::CC0,
            "CC_BY" => V0LicenseName::CCBy,
            "CC_BY_NC" => V0LicenseName::CCByNc,
            "CC_BY_SA" => V0LicenseName::CCBySa,
            "CC_BY_NC_SA" => V0LicenseName::CCByNcSa,
            "CC_BY_ND" => V0LicenseName::CCByNd,
            "CC_BY_NC_ND" => V0LicenseName::CCByNcNd,
            "Other" => V0LicenseName::Other,
            _ => return None,
        })
    }
}

/// Metadata of a VRM 0.x model.
#[derive(Debug, Clone, Default)]
pub struct Vrm0Meta {
    pub title: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub contact_information: Option<String>,
    pub reference: Option<String>,
    /// Thumbnail texture index.
    pub texture: Option<usize>,
    pub allowed_user_name: Option<V0AllowedUserName>,
    pub violent_usage_name: Option<V0UsagePermission>,
    pub sexual_usage_name: Option<V0UsagePermission>,
    pub commercial_usage_name: Option<V0CommercialUsagePermission>,
    pub other_permission_url: Option<String>,
    pub license_name: Option<V0LicenseName>,
    pub other_license_url: Option<String>,
}

/// Avatar permissions for VRM 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1AvatarPermission {
    Everyone,
    OnlyAuthor,
    OnlySeparatelyLicensedPerson,
}

impl V1AvatarPermission {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "everyone" => V1AvatarPermission::Everyone,
            "onlyAuthor" => V1AvatarPermission::OnlyAuthor,
            "onlySeparatelyLicensedPerson" => V1AvatarPermission::OnlySeparatelyLicensedPerson,
            _ => return None,
        })
    }
}

/// Commercial usage for VRM 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1CommercialUsage {
    PersonalNonProfit,
    PersonalProfit,
    Corporation,
}

impl V1CommercialUsage {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "personalNonProfit" => V1CommercialUsage::PersonalNonProfit,
            "personalProfit" => V1CommercialUsage::PersonalProfit,
            "corporation" => V1CommercialUsage::Corporation,
            _ => return None,
        })
    }
}

/// Credit notation for VRM 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1CreditNotation {
    Required,
    Unnecessary,
}

impl V1CreditNotation {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "required" => V1CreditNotation::Required,
            "unnecessary" => V1CreditNotation::Unnecessary,
            _ => return None,
        })
    }
}

/// Modification allowance for VRM 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1Modification {
    Prohibited,
    AllowModification,
    AllowModificationRedistribution,
}

impl V1Modification {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "prohibited" => V1Modification::Prohibited,
            "allowModification" => V1Modification::AllowModification,
            "allowModificationRedistribution" => V1Modification::AllowModificationRedistribution,
            _ => return None,
        })
    }
}

/// Metadata of a VRM 1.0 model.
#[derive(Debug, Clone, Default)]
pub struct Vrm1Meta {
    pub name: Option<String>,
    pub version: Option<String>,
    pub authors: Vec<String>,
    pub copyright_information: Option<String>,
    pub contact_information: Option<String>,
    pub references: Vec<String>,
    pub third_party_licenses: Option<String>,
    /// Thumbnail image index.
    pub thumbnail_image: Option<usize>,
    pub license_url: Option<String>,
    pub avatar_permission: Option<V1AvatarPermission>,
    pub allow_excessively_violent_usage: bool,
    pub allow_excessively_sexual_usage: bool,
    pub violent_usage_description: Option<String>,
    pub sexual_usage_description: Option<String>,
    pub commercial_usage: Option<V1CommercialUsage>,
    pub credit_notation: Option<V1CreditNotation>,
    pub allow_redistribution: bool,
    pub modification: Option<V1Modification>,
    pub other_license_url: Option<String>,
    pub other_permission_url: Option<String>,
}

/// Metadata of a VRM. Either VRM 0.x or VRM 1.0.
#[derive(Debug, Clone)]
pub enum VrmMeta {
    Vrm0(Vrm0Meta),
    Vrm1(Vrm1Meta),
}

impl VrmMeta {
    pub fn meta_type(&self) -> &'static str {
        match self {
            VrmMeta::Vrm0(_) => "vrm0",
            VrmMeta::Vrm1(_) => "vrm1",
        }
    }

    /// Common "title or name" accessor.
    pub fn title(&self) -> Option<&str> {
        match self {
            VrmMeta::Vrm0(m) => m.title.as_deref(),
            VrmMeta::Vrm1(m) => m.name.as_deref(),
        }
    }

    /// Common "version" accessor.
    pub fn version(&self) -> Option<&str> {
        match self {
            VrmMeta::Vrm0(m) => m.version.as_deref(),
            VrmMeta::Vrm1(m) => m.version.as_deref(),
        }
    }

    /// Common "author(s)" accessor.
    pub fn authors(&self) -> Vec<&str> {
        match self {
            VrmMeta::Vrm0(m) => m.author.as_deref().map(|a| vec![a]).unwrap_or_default(),
            VrmMeta::Vrm1(m) => m.authors.iter().map(|a| a.as_str()).collect(),
        }
    }
}
