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
uniform vec3 uCameraPos;
uniform int uDebugUv;
uniform int uAlphaMode;
uniform float uAlphaCutoff;

out vec4 fragColor;

void main() {
    if (uDebugUv == 1) {
        fragColor = vec4(vUv, 0.0, 1.0);
        return;
    }
    vec3 n = normalize(vNormal);
    if (!gl_FrontFacing) n = -n;
    vec3 base = uHasTex == 1 ? texture(uTex, vUv).rgb : vec3(1.0);
    vec3 albedo = uColor.rgb * base;

    // Global (hemisphere) light: a warm sky gradient blended against a cool
    // ground bounce by the normal direction, so every surface picks up light
    // from the whole environment instead of a single glossy direction.
    float sky = n.y * 0.5 + 0.5;
    vec3 ground = vec3(0.16, 0.17, 0.20);
    vec3 zenith = vec3(0.95, 0.92, 0.88);
    vec3 env = mix(ground, zenith, sky);

    // Soft key light from above-front with a wrapped, squared falloff so the
    // transition is gradual and nothing develops a hard highlight.
    vec3 keyDir = normalize(uLightDir);
    float ndl = dot(n, keyDir) * 0.5 + 0.5;
    float key = ndl * ndl;
    vec3 light = env * 0.65 + vec3(0.55) * key;

    vec3 col = albedo * light;

    // The default framebuffer is linear (not sRGB), so tonemap and
    // gamma-encode here instead of relying on the GL surface.
    col = col / (col + vec3(1.0));
    col = pow(col, vec3(1.0 / 2.2));

    float texAlpha = uHasTex == 1 ? texture(uTex, vUv).a : 1.0;
    if (uAlphaMode == 1) {
        // Mask: cut the surface at the alpha cutoff (hair fringes, lace).
        if (texAlpha < uAlphaCutoff) discard;
        fragColor = vec4(col, uColor.a);
    } else if (uAlphaMode == 2) {
        // Blend: keep the texture alpha for translucency.
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
    camera_pos: Option<glow::UniformLocation>,
    debug_uv: Option<glow::UniformLocation>,
    alpha_mode: Option<glow::UniformLocation>,
    alpha_cutoff: Option<glow::UniformLocation>,
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

        let materials = doc
            .materials()
            .map(|m| {
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
                }
            })
            .collect();

        let textures = upload_textures(&gl, doc, images);

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
                camera_pos: get("uCameraPos"),
                debug_uv: get("uDebugUv"),
                alpha_mode: get("uAlphaMode"),
                alpha_cutoff: get("uAlphaCutoff"),
            }
        };
        let msaa_samples = std::env::var("VRM_VIEWER_MSAA")
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(4)
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

fn upload_textures(
    gl: &glow::Context,
    doc: &gltf::Document,
    images: &[gltf::image::Data],
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
        let (format, internal) = match data.format {
            gltf::image::Format::R8 => (glow::RED, glow::R8),
            gltf::image::Format::R8G8 => (glow::RG, glow::RG8),
            gltf::image::Format::R8G8B8 => (glow::RGB, glow::SRGB8),
            gltf::image::Format::R8G8B8A8 => (glow::RGBA, glow::SRGB8_ALPHA8),
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
