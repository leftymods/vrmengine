//! Material model, including the VRM MToon material params (ported from `@pixiv/three-vrm-materials-mtoon`).

use glam::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineWidthMode {
    None,
    WorldCoordinates,
    ScreenCoordinates,
}

/// Texture slots that can carry a per-material UV transform (for expression texture-transform binds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexSlot {
    BaseColor,
    Emissive,
    Shade,
    ShadingShift,
    Matcap,
    Rim,
    OutlineWidth,
    UvAnimMask,
    Normal,
}

/// Per-slot UV transform: (scale.x, scale.y, offset.x, offset.y) applied as `uv * scale + offset`.
pub type UvTransform = [f32; 4];

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,

    pub kind: MaterialKind,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,

    /// lit factor (base color + alpha). Named like MToon's `litFactor`.
    pub color: Vec4,
    pub base_color_texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,

    pub normal_map: Option<usize>,
    pub normal_scale: f32,

    pub emissive: Vec3,
    pub emissive_intensity: f32,
    pub emissive_map: Option<usize>,

    pub unlit: bool,

    // MToon
    pub shade_color: Vec3,
    pub shade_multiply_texture: Option<usize>,
    pub shading_shift: f32,
    pub shading_toony: f32,
    pub shading_shift_texture: Option<usize>,
    pub shading_shift_texture_scale: f32,
    pub gi_equalization: f32,

    pub matcap_color: Vec3,
    pub matcap_texture: Option<usize>,

    pub rim_color: Vec3,
    pub rim_multiply_texture: Option<usize>,
    pub rim_lighting_mix: f32,
    pub rim_fresnel_power: f32,
    pub rim_lift: f32,

    pub outline_width_mode: OutlineWidthMode,
    pub outline_width: f32,
    pub outline_width_multiply_texture: Option<usize>,
    pub outline_color: Vec3,
    pub outline_lighting_mix: f32,

    pub uv_animation_mask: Option<usize>,
    pub uv_scroll_x_speed: f32,
    pub uv_scroll_y_speed: f32,
    pub uv_rotation_speed: f32,
    /// Runtime UV animation offsets, updated by `update(delta)`.
    pub uv_anim_offset: Vec2,
    pub uv_anim_rotation_phase: f32,

    pub v0_compat_shade: bool,
    pub v0_vertex_color: bool,

    /// Per-slot UV transforms mutated by `VRMExpressionTextureTransformBind`.
    pub uv_transforms: std::collections::HashMap<TexSlot, UvTransform>,
}

impl Material {
    pub fn uv_transform(&self, slot: TexSlot) -> UvTransform {
        self.uv_transforms.get(&slot).copied().unwrap_or([1.0, 1.0, 0.0, 0.0])
    }

    pub fn set_uv_transform(&mut self, slot: TexSlot, transform: UvTransform) {
        self.uv_transforms.insert(slot, transform);
    }

    pub fn update(&mut self, delta: f32) {
        self.uv_anim_offset.x += delta * self.uv_scroll_x_speed;
        self.uv_anim_offset.y += delta * self.uv_scroll_y_speed;
        self.uv_anim_rotation_phase += delta * self.uv_animation_rotation_speed();
    }

    fn uv_animation_rotation_speed(&self) -> f32 {
        self.uv_rotation_speed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialKind {
    Mtoon,
    Standard,
    Unlit,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            name: String::new(),
            kind: MaterialKind::Standard,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            color: Vec4::ONE,
            base_color_texture: None,
            metallic: 0.0,
            roughness: 1.0,
            normal_map: None,
            normal_scale: 1.0,
            emissive: Vec3::ZERO,
            emissive_intensity: 1.0,
            emissive_map: None,
            unlit: false,
            shade_color: Vec3::new(0.5, 0.5, 0.5),
            shade_multiply_texture: None,
            shading_shift: 0.0,
            shading_toony: 1.0,
            shading_shift_texture: None,
            shading_shift_texture_scale: 1.0,
            gi_equalization: 0.0,
            matcap_color: Vec3::ONE,
            matcap_texture: None,
            rim_color: Vec3::ZERO,
            rim_multiply_texture: None,
            rim_lighting_mix: 1.0,
            rim_fresnel_power: 1.0,
            rim_lift: 0.0,
            outline_width_mode: OutlineWidthMode::None,
            outline_width: 0.0,
            outline_width_multiply_texture: None,
            outline_color: Vec3::ZERO,
            outline_lighting_mix: 1.0,
            uv_animation_mask: None,
            uv_scroll_x_speed: 0.0,
            uv_scroll_y_speed: 0.0,
            uv_rotation_speed: 0.0,
            uv_anim_offset: Vec2::ZERO,
            uv_anim_rotation_phase: 0.0,
            v0_compat_shade: false,
            v0_vertex_color: false,
            uv_transforms: std::collections::HashMap::new(),
        }
    }
}
