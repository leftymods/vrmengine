//! Minimal OpenGL renderer for the VRM scene graph.
//!
//! Handles:
//! - Uploading mesh primitives (positions, normals, UVs, colors, indices) to GPU buffers.
//! - Uploading textures from `ImageData`.
//! - CPU morph + skinning every frame (via `Scene::compute_skinned`) with `glBufferSubData` updates.
//! - Simple directional diffuse shading + MToon-like shade colour.

use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4};
use glow::HasContext;

use crate::material::{AlphaMode, Material, TexSlot};
use crate::scene::{ImageData, Primitive, Scene, Texture, WrapMode};
use crate::log::{debug, info};

#[allow(dead_code)]
const MAX_BONES: usize = 128;

static VS_SRC: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec2 aTexCoord;
layout(location = 3) in vec4 aColor;

uniform mat4 u_model;
uniform mat4 u_viewProj;
uniform bool u_hasSkin;

out vec3 vWorldPos;
out vec3 vNormal;
out vec2 vTexCoord;
out vec4 vColor;

void main() {
    vec4 worldPos = u_model * vec4(aPos, 1.0);
    gl_Position = u_viewProj * worldPos;
    vWorldPos = worldPos.xyz;
    mat3 normalMatrix = mat3(transpose(inverse(u_model)));
    vNormal = normalMatrix * aNormal;
    vTexCoord = aTexCoord;
    vColor = aColor;
}
"#;

static FS_SRC: &str = r#"#version 330 core
in vec3 vWorldPos;
in vec3 vNormal;
in vec2 vTexCoord;
in vec4 vColor;

uniform vec4 u_baseColor;
uniform sampler2D u_baseTex;
uniform bool u_hasBaseTex;
uniform vec2 u_uvScale;
uniform vec2 u_uvOffset;

uniform vec3 u_lightDir;
uniform vec3 u_shadeColor;
uniform float u_shadingToony;
uniform float u_alphaCutoff;
uniform int u_alphaMode;
uniform vec3 u_emissive;
uniform bool u_unlit;

out vec4 FragColor;

void main() {
    vec2 uv = vTexCoord * u_uvScale + u_uvOffset;
    vec4 color = u_baseColor;
    if (u_hasBaseTex) color *= texture(u_baseTex, uv);
    if (vColor.a > 0.0) color.rgb *= vColor.rgb;

    vec3 N = normalize(vNormal);
    vec3 L = normalize(u_lightDir);
    float ndotl = max(dot(N, L), 0.0);

    vec3 finalColor;
    if (u_unlit) {
        finalColor = color.rgb;
    } else {
        vec3 lit = color.rgb;
        vec3 shaded = mix(u_shadeColor * lit, lit, ndotl);
        float toony = smoothstep(u_shadingToony * 0.5, u_shadingToony * 0.5 + 0.01, ndotl);
        shaded = mix(shaded, lit, toony);
        vec3 ambient = lit * 0.2;
        finalColor = shaded + ambient + u_emissive;
    }

    if (u_alphaMode == 1 && color.a < u_alphaCutoff) discard;
    FragColor = vec4(finalColor, color.a);
}
"#;

#[derive(Debug, Clone)]
pub struct GpuPrimitive {
    pub vao: glow::VertexArray,
    pub pos_vbo: glow::Buffer,
    pub norm_vbo: glow::Buffer,
    pub uv_vbo: glow::Buffer,
    pub col_vbo: glow::Buffer,
    pub ebo: Option<glow::Buffer>,
    pub index_count: i32,
    pub vertex_count: i32,
    pub material: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GpuMesh {
    pub primitives: Vec<GpuPrimitive>,
}

pub struct Renderer {
    pub program: glow::Program,
    pub gpu_meshes: Vec<GpuMesh>,
    pub textures: Vec<glow::Texture>,
    pub camera: Camera,
    light_dir: Vec3,
}

pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fovy: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 1.2, 2.5),
            target: Vec3::new(0.0, 0.9, 0.0),
            up: Vec3::Y,
            fovy: 45f32.to_radians(),
            near: 0.01,
            far: 100.0,
        }
    }
}

impl Camera {
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }
}

impl Renderer {
    pub fn new(gl: &glow::Context) -> Self {
        debug("Compiling shaders...");
        let program = unsafe { compile_program(gl, VS_SRC, FS_SRC) };
        info("Shaders compiled, renderer created");
        Self {
            program,
            gpu_meshes: Vec::new(),
            textures: Vec::new(),
            camera: Camera::default(),
            light_dir: Vec3::new(0.0, 1.0, 0.5).normalize(),
        }
    }

    pub fn upload_scene(&mut self, gl: &glow::Context, scene: &Scene) {
        debug(&format!(
            "Uploading scene: {} meshes, {} textures",
            scene.meshes.len(),
            scene.textures.len()
        ));
        self.gpu_meshes.clear();
        self.textures.clear();

        // Upload textures
        let mut image_to_tex: HashMap<usize, glow::Texture> = HashMap::new();
        for (i, tex) in scene.textures.iter().enumerate() {
            if let Some(image) = scene.images.get(tex.image) {
                let handle = unsafe { upload_texture(gl, image, tex) };
                image_to_tex.insert(i, handle);
            }
        }
        // Expand to per-texture-index list so material.texture_index maps directly.
        let max_tex = scene.textures.len();
        self.textures.resize_with(max_tex, || unsafe { gl.create_texture().unwrap() });
        for (i, _tex) in scene.textures.iter().enumerate() {
            if let Some(handle) = image_to_tex.get(&i) {
                self.textures[i] = *handle;
            }
        }

        // Upload meshes
        for mesh in &scene.meshes {
            let mut gpu_prims = Vec::new();
            for prim in &mesh.primitives {
                let gpu = unsafe { upload_primitive(gl, prim) };
                gpu_prims.push(gpu);
            }
            self.gpu_meshes.push(GpuMesh {
                primitives: gpu_prims,
            });
        }
        info(&format!(
            "Uploaded {} meshes ({} primitives) and {} textures to GPU",
            self.gpu_meshes.len(),
            self.gpu_meshes.iter().map(|m| m.primitives.len()).sum::<usize>(),
            self.textures.len()
        ));
    }

    pub fn draw(
        &mut self,
        gl: &glow::Context,
        scene: &mut Scene,
        width: u32,
        height: u32,
    ) {
        scene.update_world_matrices();

        let aspect = width as f32 / height.max(1) as f32;
        let view = self.camera.view_matrix();
        let proj = Mat4::perspective_rh_gl(self.camera.fovy, aspect, self.camera.near, self.camera.far);
        let vp = proj * view;

        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
        }

        let light_dir = self.light_dir;
        let visible = scene.visible_mesh_nodes();
        for (_node_idx, node) in visible {
            let model = node.world_matrix;
            let mesh_idx = match node.mesh {
                Some(m) => m,
                None => continue,
            };
            let gpu_mesh = match self.gpu_meshes.get(mesh_idx) {
                Some(m) => m,
                None => continue,
            };
            let mesh = &scene.meshes[mesh_idx];

            // Prepare skinning matrices if any
            let bone_matrices: Vec<Mat4> = if let Some(skin_idx) = node.skin {
                scene.bone_matrices(skin_idx)
            } else {
                Vec::new()
            };

            for (prim_idx, gpu_prim) in gpu_mesh.primitives.iter().enumerate() {
                let prim = &mesh.primitives[prim_idx];
                // CPU morph + skin
                let (pos, norm) = if !bone_matrices.is_empty() {
                    scene.compute_skinned(prim, &bone_matrices)
                } else {
                    scene.compute_morph(prim)
                };

                unsafe {
                    gl.bind_vertex_array(Some(gpu_prim.vao));
                    // update positions
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(gpu_prim.pos_vbo));
                    gl.buffer_sub_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        0,
                        bytemuck::cast_slice(&pos),
                    );
                    // update normals
                    if let Some(ref n) = norm {
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(gpu_prim.norm_vbo));
                        gl.buffer_sub_data_u8_slice(
                            glow::ARRAY_BUFFER,
                            0,
                            bytemuck::cast_slice(n),
                        );
                    }

                    set_uniform_mat4(gl, self.program, "u_viewProj", &vp);
                    set_uniform_mat4(gl, self.program, "u_model", &model);
                    set_uniform_vec3(gl, self.program, "u_lightDir", &light_dir);

                    let mat = prim.material.and_then(|m| scene.materials.get(m));
                    set_material_uniforms(gl, self.program, mat, &self.textures);

                    if let Some(_ebo) = gpu_prim.ebo {
                        gl.draw_elements(glow::TRIANGLES, gpu_prim.index_count, glow::UNSIGNED_INT, 0);
                    } else {
                        gl.draw_arrays(glow::TRIANGLES, 0, gpu_prim.vertex_count);
                    }
                }
            }
        }

        unsafe {
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
        }
    }
}

unsafe fn upload_texture(
    gl: &glow::Context,
    image: &ImageData,
    tex_info: &Texture,
) -> glow::Texture {
    let tex = gl.create_texture().unwrap();
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA as i32,
        image.width as i32,
        image.height as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(&image.rgba)),
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        tex_info.min_filter.map_or(glow::LINEAR_MIPMAP_LINEAR as i32, |f| f as i32),
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        tex_info.mag_filter.map_or(glow::LINEAR as i32, |f| f as i32),
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        wrap_gl(tex_info.wrap_s) as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        wrap_gl(tex_info.wrap_t) as i32,
    );
    gl.generate_mipmap(glow::TEXTURE_2D);
    tex
}

fn wrap_gl(mode: WrapMode) -> u32 {
    match mode {
        WrapMode::Repeat => glow::REPEAT,
        WrapMode::MirroredRepeat => glow::MIRRORED_REPEAT,
        WrapMode::ClampToEdge => glow::CLAMP_TO_EDGE,
    }
}

unsafe fn upload_primitive(gl: &glow::Context, prim: &Primitive) -> GpuPrimitive {
    let vao = gl.create_vertex_array().unwrap();
    gl.bind_vertex_array(Some(vao));

    let pos_data: Vec<f32> = prim.positions.iter().flatten().copied().collect();
    let pos_vbo = gl.create_buffer().unwrap();
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pos_vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&pos_data),
        glow::DYNAMIC_DRAW,
    );
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);

    let norm_data: Vec<f32> = prim
        .normals
        .as_ref()
        .map(|n| n.iter().flatten().copied().collect())
        .unwrap_or_else(|| vec![0.0f32; prim.vertex_count() * 3]);
    let norm_vbo = gl.create_buffer().unwrap();
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(norm_vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&norm_data),
        glow::DYNAMIC_DRAW,
    );
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 0, 0);

    let uv_data: Vec<f32> = prim
        .texcoords
        .as_ref()
        .map(|t| t.iter().flatten().copied().collect())
        .unwrap_or_else(|| vec![0.0f32; prim.vertex_count() * 2]);
    let uv_vbo = gl.create_buffer().unwrap();
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(uv_vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&uv_data),
        glow::STATIC_DRAW,
    );
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, 0, 0);

    let col_data: Vec<f32> = prim
        .colors
        .as_ref()
        .map(|c| c.iter().flatten().copied().collect())
        .unwrap_or_else(|| vec![0.0f32; prim.vertex_count() * 4]);
    let col_vbo = gl.create_buffer().unwrap();
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(col_vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&col_data),
        glow::STATIC_DRAW,
    );
    gl.enable_vertex_attrib_array(3);
    gl.vertex_attrib_pointer_f32(3, 4, glow::FLOAT, false, 0, 0);

    let (ebo, index_count) = if let Some(ref indices) = prim.indices {
        let ebo = gl.create_buffer().unwrap();
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
        gl.buffer_data_u8_slice(
            glow::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(indices),
            glow::STATIC_DRAW,
        );
        (Some(ebo), indices.len() as i32)
    } else {
        (None, 0)
    };

    gl.bind_vertex_array(None);

    GpuPrimitive {
        vao,
        pos_vbo,
        norm_vbo,
        uv_vbo,
        col_vbo,
        ebo,
        index_count,
        vertex_count: prim.vertex_count() as i32,
        material: prim.material,
    }
}

unsafe fn set_material_uniforms(
    gl: &glow::Context,
    program: glow::Program,
    material: Option<&Material>,
    textures: &[glow::Texture],
) {
    let base = Material::default();
    let mat = material.unwrap_or(&base);

    set_uniform_vec4(gl, program, "u_baseColor", &mat.color);
    set_uniform_vec3(gl, program, "u_emissive", &mat.emissive);
    set_uniform_vec3(gl, program, "u_shadeColor", &mat.shade_color);
    set_uniform_f32(gl, program, "u_shadingToony", mat.shading_toony);
    set_uniform_f32(gl, program, "u_alphaCutoff", mat.alpha_cutoff);
    set_uniform_bool(gl, program, "u_unlit", mat.unlit);
    let alpha_mode = match mat.alpha_mode {
        AlphaMode::Opaque => 0,
        AlphaMode::Mask => 1,
        AlphaMode::Blend => 2,
    };
    set_uniform_i32(gl, program, "u_alphaMode", alpha_mode);

    // UV transform
    let uv_t = mat.uv_transform(TexSlot::BaseColor);
    set_uniform_vec2(gl, program, "u_uvScale", &glam::Vec2::new(uv_t[0], uv_t[1]));
    set_uniform_vec2(gl, program, "u_uvOffset", &glam::Vec2::new(uv_t[2], uv_t[3]));

    // Base texture
    let has_tex = mat.base_color_texture.is_some()
        && mat.base_color_texture.unwrap() < textures.len();
    set_uniform_bool(gl, program, "u_hasBaseTex", has_tex);
    if has_tex {
        let tex = textures[mat.base_color_texture.unwrap()];
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        let loc = gl.get_uniform_location(program, "u_baseTex");
        gl.uniform_1_i32(loc.as_ref(), 0);
    }
}

unsafe fn compile_program(gl: &glow::Context, vs: &str, fs: &str) -> glow::Program {
    let vs_shader = compile_shader(gl, glow::VERTEX_SHADER, vs);
    let fs_shader = compile_shader(gl, glow::FRAGMENT_SHADER, fs);
    let program = gl.create_program().unwrap();
    gl.attach_shader(program, vs_shader);
    gl.attach_shader(program, fs_shader);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        panic!("Program link error: {}", gl.get_program_info_log(program));
    }
    gl.detach_shader(program, vs_shader);
    gl.detach_shader(program, fs_shader);
    gl.delete_shader(vs_shader);
    gl.delete_shader(fs_shader);
    program
}

unsafe fn compile_shader(gl: &glow::Context, ty: u32, src: &str) -> glow::Shader {
    let shader = gl.create_shader(ty).unwrap();
    gl.shader_source(shader, src);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        panic!("Shader compile error: {}", gl.get_shader_info_log(shader));
    }
    shader
}

unsafe fn set_uniform_mat4(gl: &glow::Context, program: glow::Program, name: &str, mat: &Mat4) {
    let loc = gl.get_uniform_location(program, name);
    let cols = mat.to_cols_array();
    gl.uniform_matrix_4_f32_slice(loc.as_ref(), false, &cols);
}

unsafe fn set_uniform_vec3(gl: &glow::Context, program: glow::Program, name: &str, v: &Vec3) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_3_f32_slice(loc.as_ref(), &[v.x, v.y, v.z]);
}

unsafe fn set_uniform_vec4(gl: &glow::Context, program: glow::Program, name: &str, v: &Vec4) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_4_f32_slice(loc.as_ref(), &[v.x, v.y, v.z, v.w]);
}

unsafe fn set_uniform_vec2(gl: &glow::Context, program: glow::Program, name: &str, v: &glam::Vec2) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_2_f32_slice(loc.as_ref(), &[v.x, v.y]);
}

unsafe fn set_uniform_f32(gl: &glow::Context, program: glow::Program, name: &str, v: f32) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_1_f32(loc.as_ref(), v);
}

unsafe fn set_uniform_i32(gl: &glow::Context, program: glow::Program, name: &str, v: i32) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_1_i32(loc.as_ref(), v);
}

unsafe fn set_uniform_bool(gl: &glow::Context, program: glow::Program, name: &str, v: bool) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_1_i32(loc.as_ref(), if v { 1 } else { 0 });
}
