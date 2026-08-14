//! A minimal OpenGL (3.3 core) renderer built on `glow`.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use glow::HasContext;

use crate::model::{Vertex, ViewModel, MAX_BONES};
use vrm_engine::Vrm;

const VERTEX_SHADER: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec2 aUv;
layout(location = 3) in vec4 aJoint;
layout(location = 4) in vec4 aWeight;

uniform mat4 uProj;
uniform mat4 uView;
uniform mat4 uModel;
uniform mat4 uBones[200];
uniform int uSkinned;

out vec3 vNormal;
out vec3 vWorld;
out vec2 vUv;

void main() {
    vUv = aUv;
    if (uSkinned == 1) {
        mat4 skin = mat4(0.0);
        for (int i = 0; i < 4; i++) {
            skin += aWeight[i] * uBones[int(aJoint[i])];
        }
        float wsum = aWeight[0] + aWeight[1] + aWeight[2] + aWeight[3];
        vec4 p = wsum > 0.0001 ? skin * vec4(aPos, 1.0) : vec4(aPos, 1.0);
        vNormal = mat3(skin) * aNormal;
        vWorld = p.xyz;
        gl_Position = uProj * uView * p;
    } else {
        vNormal = mat3(uModel) * aNormal;
        vec4 p = uModel * vec4(aPos, 1.0);
        vWorld = p.xyz;
        gl_Position = uProj * uView * p;
    }
}
"#;

const FRAGMENT_SHADER: &str = r#"#version 330 core
in vec3 vNormal;
in vec3 vWorld;
in vec2 vUv;

uniform vec4 uColor;
uniform sampler2D uTex;
uniform int uHasTex;
uniform vec3 uLightDir;
uniform vec3 uLightColor;
uniform int uDebugUv;
uniform int uAlphaMode;
uniform float uAlphaCutoff;
uniform sampler2D uBumpTex;
uniform int uHasBumpTex;
uniform vec2 uNormalScale;

out vec4 fragColor;

// Reconstruct a tangent frame from screen-space derivatives and perturb the
// surface normal by a tangent-space normal map (three.js `perturbNormal2Arb`),
// without requiring a tangent vertex attribute.
vec3 perturbNormal2Arb(vec2 uv, vec3 pos, vec3 surf, vec3 mapN, float face) {
    vec3 q0 = dFdx(pos);
    vec3 q1 = dFdy(pos);
    vec2 st0 = dFdx(uv);
    vec2 st1 = dFdy(uv);
    vec3 N = normalize(surf);
    vec3 q1perp = cross(q1, N);
    vec3 q0perp = cross(N, q0);
    vec3 T = q1perp * st0.x + q0perp * st1.x;
    vec3 B = q1perp * st0.y + q0perp * st1.y;
    if (length(T) == 0.0 || length(B) == 0.0) return surf;
    float det = max(dot(T, T), dot(B, B));
    float scale = det == 0.0 ? 0.0 : face * inversesqrt(det);
    return normalize(T * (mapN.x * scale) + B * (mapN.y * scale) + N * mapN.z);
}

void main() {
    if (uDebugUv == 1) {
        fragColor = vec4(vUv, 0.0, 1.0);
        return;
    }
    vec3 n = normalize(vNormal);
    if (!gl_FrontFacing) n = -n;
    vec3 base = uHasTex == 1 ? texture(uTex, vUv).rgb : vec3(1.0);
    vec3 albedo = uColor.rgb * base;
    vec3 L = normalize(uLightDir);
    float ndotl = dot(n, L);

    // Tangent-space normal map (`_BumpMap`) for fine surface detail.
    if (uHasBumpTex == 1) {
        vec3 mapN = texture(uBumpTex, vUv).xyz * 2.0 - 1.0;
        mapN.xy *= uNormalScale;
        float faceDirection = gl_FrontFacing ? 1.0 : -1.0;
        n = perturbNormal2Arb(vUv, vWorld, n, mapN, faceDirection);
        ndotl = dot(n, L);
    }

    // Two-tone cel shading: a dark shade color on the shadow side and the
    // full albedo on the lit side, split by a short smoothstep ramp on
    // dot(N, L). The shade tone is forced to ~55% of the albedo so the band
    // reads even when the model's `_ShadeTexture` equals its `_MainTex`.
    vec3 shade = albedo * 0.55;
    float ramp = smoothstep(-0.02, 0.08, ndotl);
    vec3 lit = mix(shade, albedo, ramp) * uLightColor;

    // Matte hemisphere fill (sky/ground) so the shaded side keeps form and
    // never reads as flat plastic.
    vec3 sky = vec3(0.22, 0.23, 0.26);
    vec3 ground = vec3(0.09, 0.09, 0.11);
    vec3 ambient = mix(ground, sky, n.y * 0.5 + 0.5) * albedo;

    vec3 col = lit + ambient * 0.6;

    // The default framebuffer is linear (not sRGB), so gamma-encode for
    // display. No tonemapping is applied: the cel-lit value is already in
    // display range.
    col = pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2));

    float texAlpha = uHasTex == 1 ? texture(uTex, vUv).a : 1.0;
    if (uAlphaMode == 1) {
        if (texAlpha < uAlphaCutoff) discard;
        fragColor = vec4(col, uColor.a);
    } else if (uAlphaMode == 2) {
        fragColor = vec4(col, uColor.a * texAlpha);
    } else {
        fragColor = vec4(col, uColor.a);
    }
}
"#;

#[derive(Clone, Copy)]
pub struct MaterialInfo {
    pub base_color: [f32; 4],
    pub texture: Option<usize>,
    /// 0 = Opaque, 1 = Mask, 2 = Blend (from `alphaMode`).
    pub alpha_mode: u8,
    pub alpha_cutoff: f32,
    /// MToon shading parameters when the material is a `VRM/MToon` shader.
    pub mtoon: Option<MToonParams>,
}

/// Parameters of the `VRM/MToon` shader (VRM 0.x), mirroring the properties in
/// `extensions.VRM.materialProperties`. See three-vrm's `mtoon.frag` for the
/// reference implementation.
#[derive(Clone, Copy)]
pub struct MToonParams {
    pub shade_color: [f32; 3],
    pub shade_shift: f32,
    pub shade_toony: f32,
    pub shading_grade_rate: f32,
    pub rim_color: [f32; 3],
    pub rim_power: f32,
    pub rim_lift: f32,
    pub rim_mix: f32,
    pub indirect_light: f32,
    pub emission_color: [f32; 3],
    /// Image indices into the glTF images (texture -> source image resolved).
    pub shade_tex: Option<usize>,
    pub sphere_tex: Option<usize>,
    pub emission_tex: Option<usize>,
    /// `_BumpMap` tangent-space normal map (uploaded as linear, not sRGB).
    pub bump_tex: Option<usize>,
}

unsafe fn bytes_of<T: Copy>(values: &[T]) -> &[u8] {
    std::slice::from_raw_parts(
        values.as_ptr() as *const u8,
        std::mem::size_of_val(values),
    )
}

#[derive(Default)]
struct Uniforms {
    proj: Option<glow::UniformLocation>,
    view: Option<glow::UniformLocation>,
    model: Option<glow::UniformLocation>,
    bones: Option<glow::UniformLocation>,
    skinned: Option<glow::UniformLocation>,
    color: Option<glow::UniformLocation>,
    has_tex: Option<glow::UniformLocation>,
    light_dir: Option<glow::UniformLocation>,
    key_color: Option<glow::UniformLocation>,
    camera_pos: Option<glow::UniformLocation>,
    debug_uv: Option<glow::UniformLocation>,
    alpha_mode: Option<glow::UniformLocation>,
    alpha_cutoff: Option<glow::UniformLocation>,
    mtoon: Option<glow::UniformLocation>,
    shade_color: Option<glow::UniformLocation>,
    has_shade_tex: Option<glow::UniformLocation>,
    shade_shift: Option<glow::UniformLocation>,
    shade_toony: Option<glow::UniformLocation>,
    shading_grade_rate: Option<glow::UniformLocation>,
    rim_color: Option<glow::UniformLocation>,
    rim_power: Option<glow::UniformLocation>,
    rim_lift: Option<glow::UniformLocation>,
    rim_mix: Option<glow::UniformLocation>,
    indirect_light: Option<glow::UniformLocation>,
    emission_color: Option<glow::UniformLocation>,
    has_emission_tex: Option<glow::UniformLocation>,
    has_sphere_tex: Option<glow::UniformLocation>,
    shade_tex_sampler: Option<glow::UniformLocation>,
    sphere_tex_sampler: Option<glow::UniformLocation>,
    emission_tex_sampler: Option<glow::UniformLocation>,
    bump_tex_sampler: Option<glow::UniformLocation>,
    has_bump_tex: Option<glow::UniformLocation>,
    normal_scale: Option<glow::UniformLocation>,
    light_color: Option<glow::UniformLocation>,
}

pub struct Renderer {
    gl: Arc<glow::Context>,
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    u: Uniforms,
    materials: Vec<MaterialInfo>,
    textures: HashMap<usize, glow::Texture>,
    pub background: [f32; 3],
    // Multisample (MSAA) render target. When samples > 1 the scene is drawn
    // into these attachments and resolved into the default framebuffer, so
    // antialiasing works for both the window and the headless pbuffer.
    msaa_samples: i32,
    msaa_fbo: Option<glow::Framebuffer>,
    msaa_color: Option<glow::Renderbuffer>,
    msaa_depth: Option<glow::Renderbuffer>,
    fb_size: (u32, u32),
}

impl Renderer {
    pub fn new(
        gl: Arc<glow::Context>,
        doc: &gltf::Document,
        images: &[gltf::image::Data],
        material_properties: &[vrm_spec::vrm_0_0::VRMMaterial],
    ) -> Self {
        let program = unsafe { compile_program(&gl, VERTEX_SHADER, FRAGMENT_SHADER) };
        let vao = unsafe { gl.create_vertex_array() }.expect("vao");
        let vbo = unsafe { gl.create_buffer() }.expect("vbo");
        let ebo = unsafe { gl.create_buffer() }.expect("ebo");

        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));

            let stride = std::mem::size_of::<Vertex>() as i32;
            let offset = |n: usize| (n * std::mem::size_of::<f32>()) as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, offset(0));
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, offset(3));
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, offset(6));
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 4, glow::FLOAT, false, stride, offset(8));
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_pointer_f32(4, 4, glow::FLOAT, false, stride, offset(12));
            gl.bind_vertex_array(None);
        }

        let materials: Vec<MaterialInfo> = doc
            .materials()
            .enumerate()
            .map(|(i, m)| {
                let pbr = m.pbr_metallic_roughness();
                MaterialInfo {
                    base_color: pbr.base_color_factor(),
                    texture: pbr
                        .base_color_texture()
                        .map(|t| t.texture().source().index()),
                    alpha_mode: match m.alpha_mode() {
                        gltf::material::AlphaMode::Opaque => 0u8,
                        gltf::material::AlphaMode::Mask => 1u8,
                        gltf::material::AlphaMode::Blend => 2u8,
                    },
                    alpha_cutoff: m.alpha_cutoff().unwrap_or(0.5),
                    mtoon: parse_mtoon(material_properties.get(i), doc),
                }
            })
            .collect();
        let mtoon_count = materials.iter().filter(|m| m.mtoon.is_some()).count();
        println!(
            "renderer: {mtoon_count}/{} materials use VRM/MToon",
            materials.len()
        );

        let textures = {
        // The `_BumpMap` images hold tangent-space normals, not colour, so
        // they must be uploaded as linear (no sRGB decode).
        let mut linear_images = std::collections::HashSet::new();
        for m in &materials {
            if let Some(p) = &m.mtoon {
                if let Some(i) = p.bump_tex {
                    linear_images.insert(i);
                }
            }
        }
        upload_textures(&gl, doc, images, &linear_images)
    };

        let u = {
            let get = |name: &str| unsafe { gl.get_uniform_location(program, name) };
            Uniforms {
                proj: get("uProj"),
                view: get("uView"),
                model: get("uModel"),
                bones: get("uBones"),
                skinned: get("uSkinned"),
                color: get("uColor"),
                has_tex: get("uHasTex"),
                light_dir: get("uLightDir"),
                key_color: get("uKeyColor"),
                camera_pos: get("uCameraPos"),
                debug_uv: get("uDebugUv"),
                alpha_mode: get("uAlphaMode"),
                alpha_cutoff: get("uAlphaCutoff"),
                mtoon: get("uMtoon"),
                shade_color: get("uShadeColor"),
                has_shade_tex: get("uHasShadeTex"),
                shade_shift: get("uShadeShift"),
                shade_toony: get("uShadeToony"),
                shading_grade_rate: get("uShadingGradeRate"),
                rim_color: get("uRimColor"),
                rim_power: get("uRimPower"),
                rim_lift: get("uRimLift"),
                rim_mix: get("uRimMix"),
                indirect_light: get("uIndirectLight"),
                emission_color: get("uEmissionColor"),
                has_emission_tex: get("uHasEmissionTex"),
                has_sphere_tex: get("uHasSphereTex"),
                shade_tex_sampler: get("uShadeTex"),
                sphere_tex_sampler: get("uSphereTex"),
                emission_tex_sampler: get("uEmissionTex"),
                bump_tex_sampler: get("uBumpTex"),
                has_bump_tex: get("uHasBumpTex"),
                normal_scale: get("uNormalScale"),
                light_color: get("uLightColor"),
            }
        };
        println!(
            "uniforms: mtoon={} shade_color={} shade_shift={} shade_toony={} grade={} light_color={} key_color={} has_shade={} rim={} emiss={} sphere={} indirect={} bump={}",
            u.mtoon.is_some(),
            u.shade_color.is_some(),
            u.shade_shift.is_some(),
            u.shade_toony.is_some(),
            u.shading_grade_rate.is_some(),
            u.light_color.is_some(),
            u.key_color.is_some(),
            u.has_shade_tex.is_some(),
            u.rim_color.is_some(),
            u.emission_color.is_some(),
            u.has_sphere_tex.is_some(),
            u.indirect_light.is_some(),
            u.has_bump_tex.is_some(),
        );
        let msaa_samples = std::env::var("VRM_VIEWER_MSAA")
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(16)
            .max(0);
        let max_samples = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
        let msaa_samples = if msaa_samples > 1 { msaa_samples.min(max_samples) } else { 0 };
        let (msaa_fbo, msaa_color, msaa_depth) = if msaa_samples > 1 {
            unsafe {
                let fbo = gl.create_framebuffer().expect("msaa fbo");
                let color = gl.create_renderbuffer().expect("msaa color");
                let depth = gl.create_renderbuffer().expect("msaa depth");
                (Some(fbo), Some(color), Some(depth))
            }
        } else {
            (None, None, None)
        };
        let renderer = Self {
            gl,
            program,
            vao,
            vbo,
            ebo,
            u,
            materials,
            textures,
            background: [0.12, 0.14, 0.17],
            msaa_samples,
            msaa_fbo,
            msaa_color,
            msaa_depth,
            fb_size: (0, 0),
        };
        unsafe {
            renderer
                .gl
                .enable(glow::DEPTH_TEST);
            renderer.gl.depth_func(glow::LEQUAL);
            // VRoid models are exported with inconsistent triangle winding
            // (the front-face winding convention differs per triangle), so a
            // single culled pass would drop roughly half of every surface (and
            // the whole iris). Render each mesh twice, once per winding, and
            // let the depth test keep closed shells opaque.
            renderer.gl.enable(glow::CULL_FACE);
            renderer.gl.cull_face(glow::BACK);
            renderer.gl.enable(glow::BLEND);
            renderer
                .gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }
        renderer
    }

    /// Access the underlying GL context (e.g. to read back rendered pixels).
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// A shared handle to the GL context, for use by `egui_glow`.
    pub fn gl_arc(&self) -> Arc<glow::Context> {
        self.gl.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(        &mut self,
        vrm: &Vrm,
        model: &ViewModel,
        view: Mat4,
        proj: Mat4,
        camera_pos: Vec3,
        width: u32,
        height: u32,
    ) {
        if self.msaa_samples > 1 {
            unsafe { self.prepare_msaa(width, height) };
        }
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, width as i32, height as i32);
            if self.msaa_samples > 1 {
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.msaa_fbo);
            }
            // Reset the full GL state each frame: the egui overlay painted
            // after the previous scene disables the depth test, enables the
            // scissor test and switches blending to a premultiplied-alpha
            // mode, and we must not inherit any of that.
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(true, true, true, true);
            gl.enable(glow::CULL_FACE);
            gl.cull_face(glow::BACK);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.clear_color(
                self.background[0],
                self.background[1],
                self.background[2],
                1.0,
            );
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.use_program(Some(self.program));
        }
        self.set_mat4(self.u.proj, &proj);
        self.set_mat4(self.u.view, &view);
        self.set_vec3(self.u.light_dir, &Vec3::new(0.4, 0.8, 0.5));
        self.set_vec3(self.u.key_color, &Vec3::splat(0.55));
        self.set_vec3(self.u.light_color, &Vec3::splat(1.0));
        self.set_vec3(self.u.camera_pos, &camera_pos);
        let debug_uv = std::env::var("VRM_VIEWER_DEBUG_UV").is_ok() as i32;
        unsafe {
            gl.uniform_1_i32(self.u.debug_uv.as_ref(), debug_uv);
        }

        let mut draws: Vec<usize> = (0..model.meshes.len()).collect();
        if let Ok(filter) = std::env::var("VRM_VIEWER_MAT_FILTER") {
            let mats: Vec<String> = filter.split(',').map(str::to_string).collect();
            draws.retain(|&i| mats.iter().any(|m| model.meshes[i].material.to_string() == *m));
        }
        if std::env::var("VRM_VIEWER_DEBUG_DRAW").is_ok() {
            for &i in &draws {
                let m = &model.meshes[i];
                eprintln!(
                    "draw node={} mat={} verts={} idx={} skin={} alpha={} ds={} tex={:?}",
                    m.node,
                    m.material,
                    m.vertices.len(),
                    m.indices.len(),
                    m.skin.is_some(),
                    m.alpha,
                    m.double_sided,
                    self.materials[m.material].texture
                );
            }
        }
        // Painter's algorithm: draw far surfaces first, nearest last, so
        // coplanar overlays (VRoid merged face/body) render in the right order.
        // True BLEND materials are drawn last. `alpha` is the transparency bucket.
        let depth_key = |i: usize| -> f32 {
            let mesh = &model.meshes[i];
            let m = vrm.world_matrix(mesh.node);
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for v in &mesh.vertices {
                let p = Vec3::new(v.pos[0], v.pos[1], v.pos[2]);
                min = min.min(p);
                max = max.max(p);
            }
            let corners = [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(max.x, max.y, max.z),
            ];
            let mut nearest = f32::NEG_INFINITY;
            for c in corners {
                let wz = view.transform_point3(m.transform_point3(c)).z;
                if wz > nearest {
                    nearest = wz;
                }
            }
            nearest
        };
        draws.sort_by(|&a, &b| {
            model.meshes[a]
                .alpha
                .cmp(&model.meshes[b].alpha)
                .then_with(|| depth_key(a).partial_cmp(&depth_key(b)).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| b.cmp(&a))
        });

        let mut bone_buffer = Vec::with_capacity(MAX_BONES * 16);
        for &index in &draws {
            let mesh = &model.meshes[index];
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            let material = self.materials[mesh.material];
            let model_matrix = match &mesh.skin {
                Some(skin) => {
                    bone_buffer.clear();
                    for m in &skin.matrices {
                        bone_buffer.extend_from_slice(&m.to_cols_array());
                    }
                    bone_buffer.resize(MAX_BONES * 16, 0.0);
                    unsafe {
                        gl.uniform_matrix_4_f32_slice(
                            self.u.bones.as_ref(),
                            false,
                            &bone_buffer,
                        );
                    }
                    self.set_i32(self.u.skinned, 1);
                    Mat4::IDENTITY
                }
                None => {
                    self.set_i32(self.u.skinned, 0);
                    vrm.world_matrix(mesh.node)
                }
            };
            self.set_mat4(self.u.model, &model_matrix);
            self.set_vec4(self.u.color, &material.base_color);
            self.set_i32(self.u.alpha_mode, material.alpha_mode as i32);
            self.set_f32(self.u.alpha_cutoff, material.alpha_cutoff);

            // Upload vertex data (morph targets are baked on the CPU).
            unsafe {
                gl.bind_vertex_array(Some(self.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytes_of(&mesh.vertices),
                    glow::DYNAMIC_DRAW,
                );
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
                gl.buffer_data_u8_slice(
                    glow::ELEMENT_ARRAY_BUFFER,
                    bytes_of(&mesh.indices),
                    glow::DYNAMIC_DRAW,
                );
            }

            if let Some(texture_index) = material.texture {
                if let Some(texture) = self.textures.get(&texture_index) {
                    unsafe {
                        gl.active_texture(glow::TEXTURE0);
                        gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                    }
                    self.set_i32(self.u.has_tex, 1);
                } else {
                    self.set_i32(self.u.has_tex, 0);
                }
            } else {
                self.set_i32(self.u.has_tex, 0);
            }

            // MToon shading parameters + extra samplers (shade / matcap /
            // emission). All images are already uploaded in `textures`.
            let mtoon = material.mtoon;
            if let Some(p) = mtoon {
                self.set_i32(self.u.mtoon, 1);
                self.set_vec3(self.u.shade_color, &Vec3::from(p.shade_color));
                self.set_f32(self.u.shade_shift, p.shade_shift);
                self.set_f32(self.u.shade_toony, p.shade_toony);
                self.set_f32(self.u.shading_grade_rate, p.shading_grade_rate);
                self.set_vec3(self.u.rim_color, &Vec3::from(p.rim_color));
                self.set_f32(self.u.rim_power, p.rim_power);
                self.set_f32(self.u.rim_lift, p.rim_lift);
                self.set_f32(self.u.rim_mix, p.rim_mix);
                self.set_f32(self.u.indirect_light, p.indirect_light);
                self.set_vec3(self.u.emission_color, &Vec3::from(p.emission_color));
                unsafe {
                    // Bind an MToon texture to a given unit, set its presence
                    // flag, and point its `sampler2D` uniform at that unit so
                    // it actually samples the bound texture (the samplers
                    // otherwise default to unit 0, i.e. the main texture).
                    let bind = |unit: u32,
                                image: Option<usize>,
                                has: Option<&glow::UniformLocation>,
                                sampler: Option<&glow::UniformLocation>| {
                        match image.and_then(|i| self.textures.get(&i)) {
                            Some(tex) => {
                                gl.active_texture(unit);
                                gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
                                gl.uniform_1_i32(has, 1);
                                gl.uniform_1_i32(sampler, unit as i32);
                            }
                            None => gl.uniform_1_i32(has, 0),
                        }
                    };
                    bind(
                        glow::TEXTURE1,
                        p.shade_tex,
                        self.u.has_shade_tex.as_ref(),
                        self.u.shade_tex_sampler.as_ref(),
                    );
                    bind(
                        glow::TEXTURE2,
                        p.sphere_tex,
                        self.u.has_sphere_tex.as_ref(),
                        self.u.sphere_tex_sampler.as_ref(),
                    );
                    bind(
                        glow::TEXTURE3,
                        p.emission_tex,
                        self.u.has_emission_tex.as_ref(),
                        self.u.emission_tex_sampler.as_ref(),
                    );
                    bind(
                        glow::TEXTURE4,
                        p.bump_tex,
                        self.u.has_bump_tex.as_ref(),
                        self.u.bump_tex_sampler.as_ref(),
                    );
                    gl.uniform_2_f32(self.u.normal_scale.as_ref(), 1.0, 1.0);
                    gl.active_texture(glow::TEXTURE0);
                }
            } else {
                self.set_i32(self.u.mtoon, 0);
            }

            // Draw both windings (VRoid exports mixed winding): pass 1 draws
            // back-facing (CCW) triangles, pass 2 the front-facing (CW) ones.
            // The depth buffer keeps closed shells from showing through.
            for cull in [glow::FRONT, glow::BACK] {
                unsafe {
                    gl.cull_face(cull);
                    gl.draw_elements(
                        glow::TRIANGLES,
                        mesh.indices.len() as i32,
                        glow::UNSIGNED_INT,
                        0,
                    );
                }
            }
            unsafe {
                gl.bind_vertex_array(None);
            }
        }
        unsafe {
            gl.use_program(None);
        }
        if self.msaa_samples > 1 {
            // Resolve the multisampled target into the default framebuffer.
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, self.msaa_fbo);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                gl.blit_framebuffer(
                    0,
                    0,
                    width as i32,
                    height as i32,
                    0,
                    0,
                    width as i32,
                    height as i32,
                    glow::COLOR_BUFFER_BIT,
                    glow::LINEAR,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            }
        }
    }

    /// (Re)allocate the multisample attachments when the surface resizes.
    unsafe fn prepare_msaa(&mut self, width: u32, height: u32) {
        let gl = &self.gl;
        let (w, h) = self.fb_size;
        if (w, h) == (width, height) {
            return;
        }
        let (Some(fbo), Some(color), Some(depth)) = (self.msaa_fbo, self.msaa_color, self.msaa_depth)
        else {
            return;
        };
        self.fb_size = (width, height);
        let w = width.max(1) as i32;
        let h = height.max(1) as i32;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER,
            self.msaa_samples,
            glow::RGBA8,
            w,
            h,
        );
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER,
            self.msaa_samples,
            glow::DEPTH_COMPONENT24,
            w,
            h,
        );
        gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            Some(color),
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth),
        );
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        debug_assert_eq!(status, glow::FRAMEBUFFER_COMPLETE);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }

    fn set_mat4(&self, location: Option<glow::UniformLocation>, matrix: &Mat4) {
        unsafe {
            self.gl
                .uniform_matrix_4_f32_slice(location.as_ref(), false, &matrix.to_cols_array());
        }
    }

    fn set_vec3(&self, location: Option<glow::UniformLocation>, v: &Vec3) {
        unsafe {
            self.gl
                .uniform_3_f32(location.as_ref(), v.x, v.y, v.z);
        }
    }

    fn set_vec4(&self, location: Option<glow::UniformLocation>, v: &[f32; 4]) {
        unsafe {
            self.gl
                .uniform_4_f32(location.as_ref(), v[0], v[1], v[2], v[3]);
        }
    }

    fn set_i32(&self, location: Option<glow::UniformLocation>, value: i32) {
        unsafe {
            self.gl.uniform_1_i32(location.as_ref(), value);
        }
    }

    fn set_f32(&self, location: Option<glow::UniformLocation>, value: f32) {
        unsafe {
            self.gl.uniform_1_f32(location.as_ref(), value);
        }
    }
}

unsafe fn compile_program(gl: &glow::Context, vs: &str, fs: &str) -> glow::Program {
    unsafe fn compile_shader(gl: &glow::Context, ty: u32, source: &str) -> glow::Shader {
        let shader = gl.create_shader(ty).expect("shader");
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            panic!("shader error: {}", gl.get_shader_info_log(shader));
        }
        shader
    }

    let vertex = compile_shader(gl, glow::VERTEX_SHADER, vs);
    let fragment = compile_shader(gl, glow::FRAGMENT_SHADER, fs);
    let program = gl.create_program().expect("program");
    gl.attach_shader(program, vertex);
    gl.attach_shader(program, fragment);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        panic!("link error: {}", gl.get_program_info_log(program));
    }
    gl.detach_shader(program, vertex);
    gl.detach_shader(program, fragment);
    gl.delete_shader(vertex);
    gl.delete_shader(fragment);
    program
}

/// Resolve a `_MainTex`-style texture property (a glTF texture index) to its
/// source image index, so it can be looked up in the uploaded texture map.
fn mtoon_image(
    props: &vrm_spec::vrm_0_0::VRMMaterial,
    name: &str,
    doc: &gltf::Document,
) -> Option<usize> {
    let idx = props
        .texture_properties
        .as_ref()
        .and_then(|m| m.get(name))
        .copied()?;
    let tex: usize = idx.value();
    doc.textures().nth(tex).map(|t| t.source().index())
}

/// Extract `VRM/MToon` shading parameters from a VRM 0.0 material property.
/// Returns `None` for non-MToon materials so the PBR path stays untouched.
fn parse_mtoon(props: Option<&vrm_spec::vrm_0_0::VRMMaterial>, doc: &gltf::Document) -> Option<MToonParams> {
    let props = props?;
    if props.shader.as_deref() != Some("VRM/MToon") {
        return None;
    }
    let f = |name: &str| props.float_properties.as_ref().and_then(|m| m.get(name)).copied();
    let v = |name: &str| {
        props
            .vector_properties
            .as_ref()
            .and_then(|m| m.get(name))
            .map(|x| [x.first().copied().unwrap_or(0.0), x.get(1).copied().unwrap_or(0.0), x.get(2).copied().unwrap_or(0.0)])
    };
    let to3 = |c: Option<[f64; 3]>| c.map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]);
    Some(MToonParams {
        shade_color: to3(v("_ShadeColor")).unwrap_or([1.0, 1.0, 1.0]),
        shade_shift: f("_ShadeShift").unwrap_or(0.0) as f32,
        shade_toony: f("_ShadeToony").unwrap_or(0.9) as f32,
        shading_grade_rate: f("_ShadingGradeRate").unwrap_or(1.0) as f32,
        rim_color: to3(v("_RimColor")).unwrap_or([0.0, 0.0, 0.0]),
        rim_power: f("_RimFresnelPower").unwrap_or(1.0) as f32,
        rim_lift: f("_RimLift").unwrap_or(0.0) as f32,
        rim_mix: f("_RimLightingMix").unwrap_or(0.0) as f32,
        indirect_light: f("_IndirectLightIntensity").unwrap_or(0.1) as f32,
        emission_color: to3(v("_EmissionColor")).unwrap_or([0.0, 0.0, 0.0]),
        shade_tex: mtoon_image(props, "_ShadeTexture", doc),
        sphere_tex: mtoon_image(props, "_SphereAdd", doc),
        emission_tex: mtoon_image(props, "_EmissionMap", doc),
        bump_tex: mtoon_image(props, "_BumpMap", doc),
    })
}

fn upload_textures(
    gl: &glow::Context,
    doc: &gltf::Document,
    images: &[gltf::image::Data],
    linear_images: &std::collections::HashSet<usize>,
) -> HashMap<usize, glow::Texture> {
    let mut textures = HashMap::new();
    for image in doc.images() {
        let Some(data) = images.get(image.index()) else {
            continue;
        };
        if data.pixels.is_empty() || data.width == 0 || data.height == 0 {
            continue;
        }
        let texture = unsafe { gl.create_texture() }.expect("texture");
        // Normal maps / data textures must be sampled as linear; colour
        // textures (albedo, shade, sphere, emission) keep sRGB so sampling
        // returns linear values directly.
        let is_linear = linear_images.contains(&image.index());
        let (format, internal) = match (data.format, is_linear) {
            (gltf::image::Format::R8, _) => (glow::RED, glow::R8),
            (gltf::image::Format::R8G8, _) => (glow::RG, glow::RG8),
            (gltf::image::Format::R8G8B8, true) => (glow::RGB, glow::RGB8),
            (gltf::image::Format::R8G8B8, false) => (glow::RGB, glow::SRGB8),
            (gltf::image::Format::R8G8B8A8, true) => (glow::RGBA, glow::RGBA8),
            (gltf::image::Format::R8G8B8A8, false) => (glow::RGBA, glow::SRGB8_ALPHA8),
            _ => continue,
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal as i32,
                data.width as i32,
                data.height as i32,
                0,
                format,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&data.pixels)),
            );
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR_MIPMAP_LINEAR as i32,
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::REPEAT as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::REPEAT as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        textures.insert(image.index(), texture);
    }
    textures
}
