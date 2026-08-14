//! A minimal OpenGL (3.3 core) renderer built on `glow`.

use std::collections::HashMap;

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

out vec4 fragColor;

void main() {
    vec3 n = normalize(vNormal);
    if (!gl_FrontFacing) n = -n;
    vec3 base = uHasTex == 1 ? texture(uTex, vUv).rgb : vec3(1.0);
    vec3 albedo = uColor.rgb * base;
    vec3 light = normalize(uLightDir);
    float diff = max(dot(n, light), 0.0);
    vec3 view = normalize(uCameraPos - vWorld);
    vec3 halfway = normalize(light + view);
    float spec = pow(max(dot(n, halfway), 0.0), 32.0);
    vec3 col = albedo * (0.35 + 0.65 * diff) + vec3(spec) * 0.35;
    fragColor = vec4(col, uColor.a);
}
"#;

#[derive(Clone, Copy)]
pub struct MaterialInfo {
    pub base_color: [f32; 4],
    pub texture: Option<usize>,
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
}

pub struct Renderer {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    u: Uniforms,
    materials: Vec<MaterialInfo>,
    textures: HashMap<usize, glow::Texture>,
    pub background: [f32; 3],
}

impl Renderer {
    pub fn new(
        gl: glow::Context,
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
            }
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
        };
        unsafe {
            renderer
                .gl
                .enable(glow::DEPTH_TEST);
            renderer.gl.depth_func(glow::LEQUAL);
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
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, width as i32, height as i32);
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

        let mut draws: Vec<usize> = (0..model.meshes.len()).collect();
        draws.sort_by_key(|&i| model.meshes[i].alpha);

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

            if mesh.double_sided {
                unsafe {
                    gl.disable(glow::CULL_FACE);
                }
            } else {
                unsafe {
                    gl.enable(glow::CULL_FACE);
                }
            }

            unsafe {
                gl.draw_elements(
                    glow::TRIANGLES,
                    mesh.indices.len() as i32,
                    glow::UNSIGNED_INT,
                    0,
                );
                gl.bind_vertex_array(None);
            }
        }
        unsafe {
            gl.use_program(None);
        }
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
