//! A minimal OpenGL viewer for VRM models.
//!
//! Usage: `cargo run -p vrm-viewer -- <model.vrm>`
//!
//! Controls:
//! - drag with left mouse button: orbit
//! - drag with right mouse button: pan
//! - scroll wheel: zoom
//! - expression sliders in the "Expressions" panel
//! - `1`..`=` : set a facial expression (happy, angry, sad, surprised,
//!   relaxed, blink, neutral, aa, ih, ou, oh, ee)
//! - `r`      : reset pose and expressions
//! - `Esc`    : quit

mod camera;
mod model;
mod renderer;

use std::time::Instant;

use glow::HasContext;
use glutin::{
    config::{ConfigSurfaceTypes, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{PbufferSurface, Surface, SurfaceAttributesBuilder, WindowSurface},
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowId};

use camera::Camera;
use model::ViewModel;
use renderer::Renderer;
use vrm_engine::{ExpressionId, ExpressionPreset, LoadedModel};

struct Viewer {
    window: Window,
    _display: Display,
    _context: glutin::context::PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    renderer: Renderer,
    view_model: ViewModel,
    model: LoadedModel,
    camera: Camera,
    size: (u32, u32),
    last_frame: Instant,
    dragging: Option<MouseButton>,
    last_cursor: (f64, f64),
    fps_frames: u32,
    fps_time: f32,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_painter: egui_glow::Painter,
}

impl Viewer {
    fn new(event_loop: &ActiveEventLoop, model: LoadedModel) -> Self {
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("vrm-viewer")
                    .with_inner_size(LogicalSize::new(1280.0, 720.0)),
            )
            .expect("window");

        let raw_window = window.window_handle().unwrap().as_raw();
        let raw_display = window.display_handle().unwrap().as_raw();

        #[cfg(target_os = "windows")]
        let preference = DisplayApiPreference::Wgl(raw_window);
        #[cfg(not(target_os = "windows"))]
        let preference = DisplayApiPreference::Egl;

        let gl_display = unsafe { Display::new(raw_display, preference) }.expect("gl display");

        let config_template = ConfigTemplateBuilder::new()
            .with_depth_size(24)
            .with_alpha_size(8);
        let config = unsafe {
            gl_display
                .find_configs(config_template.build())
                .expect("no configs")
                .find(|c| c.depth_size() > 0)
                .expect("no config with depth buffer")
        };

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(glutin::context::Version::new(3, 3))))
            .build(Some(raw_window));
        let context = unsafe {
            gl_display
                .create_context(&config, &context_attributes)
                .expect("context")
        };

        let size = window.inner_size();
        let width = std::num::NonZeroU32::new(size.width).unwrap();
        let height = std::num::NonZeroU32::new(size.height).unwrap();
        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window, width, height);
        let surface = unsafe {
            gl_display
                .create_window_surface(&config, &surface_attributes)
                .expect("surface")
        };
        let context = context.make_current(&surface).expect("make current");

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                let c = std::ffi::CString::new(s).unwrap();
                gl_display.get_proc_address(&c)
            })
        };

        let view_model = model::extract(&model);
        let mut camera = Camera::default();
        camera.frame(view_model.aabb_min, view_model.aabb_max);

        let renderer = Renderer::new(std::sync::Arc::new(gl), &model.vrm.doc, &model.images, &model.vrm.material_properties);
        window.set_title(&format!(
            "vrm-viewer - {}",
            model_path(&model)
        ));

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window.display_handle().unwrap(),
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let egui_painter = egui_glow::Painter::new(renderer.gl_arc(), "", None, true)
            .expect("egui painter");

        Self {
            window,
            _display: gl_display,
            _context: context,
            surface,
            renderer,
            view_model,
            model,
            camera,
            size: (size.width, size.height),
            last_frame: Instant::now(),
            dragging: None,
            last_cursor: (0.0, 0.0),
            fps_frames: 0,
            fps_time: 0.0,
            egui_ctx,
            egui_state,
            egui_painter,
        }
    }

    fn render_frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        step_and_render(
            &mut self.model,
            &mut self.view_model,
            &self.camera,
            &mut self.renderer,
            self.size,
            dt,
        );
        self.paint_ui();
        self.surface.swap_buffers(&self._context).ok();

        self.fps_frames += 1;
        self.fps_time += dt;
        if self.fps_time >= 0.5 {
            let fps = self.fps_frames as f32 / self.fps_time;
            self.window.set_title(&format!(
                "vrm-viewer - {} - {:.0} fps",
                model_path(&self.model),
                fps
            ));
            self.fps_frames = 0;
            self.fps_time = 0.0;
        }
    }

    fn apply_expression(&mut self, preset: ExpressionPreset, weight: f32) {
        self.model
            .vrm
            .set_expression(&vrm_engine::ExpressionId::Preset(preset), weight);
        self.model.vrm.apply_expressions();
    }

    /// Render the egui overlay (expression sliders) on top of the 3D scene.
    ///
    /// Must be called with the default framebuffer bound, before
    /// `swap_buffers`; `Renderer::render` resolves its MSAA target into that
    /// framebuffer, and the egui painter draws on top of it.
    fn paint_ui(&mut self) {
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let mut changes: Vec<(ExpressionId, f32)> = Vec::new();
        let mut reset = false;

        let mut full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            egui::Window::new("Expressions")
                .default_pos([8.0, 8.0])
                .show(ui.ctx(), |ui| {
                    for id in self.model.vrm.expressions.ids() {
                        let name = id.name();
                        let mut w = self.model.vrm.expression_weight(id);
                        if ui
                            .add(egui::Slider::new(&mut w, 0.0..=1.0).text(name))
                            .changed()
                        {
                            changes.push((id.clone(), w));
                        }
                    }
                    ui.separator();
                    if ui.button("Reset all expressions").clicked() {
                        reset = true;
                    }
                    ui.label("LMB orbit · RMB pan · wheel zoom");
                    ui.label("1-0/-= expressions · r reset · Esc quit");
                });
        });

        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);
        let clipped = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let (width, height) = self.size;
        self.egui_painter.paint_and_update_textures(
            [width, height],
            full_output.pixels_per_point,
            &clipped,
            &mut full_output.textures_delta,
        );

        if reset {
            self.model.vrm.reset_expressions();
        } else {
            for (id, weight) in changes {
                self.model.vrm.set_expression(&id, weight);
            }
            self.model.vrm.apply_expressions();
        }
    }
}

fn model_path(model: &LoadedModel) -> String {
    model
        .vrm
        .meta
        .name
        .clone()
        .unwrap_or_else(|| format!("vrm {}", model.vrm.version))
}

/// Advance the simulation by `dt` seconds and draw a frame.
fn step_and_render(
    model: &mut LoadedModel,
    view_model: &mut ViewModel,
    camera: &Camera,
    renderer: &mut Renderer,
    size: (u32, u32),
    dt: f32,
) {
    let eye = camera.eye();
    model.vrm.update_spring_bones(dt);
    model.vrm.update_look_at(eye);
    model.vrm.update_transforms();

    view_model.apply_morph(&model.vrm);
    view_model.update_skins(&model.vrm);

    let view = camera.view();
    let proj = camera.proj(size.0 as f32 / size.1.max(1) as f32);
    renderer.render(&model.vrm, view_model, view, proj, eye, size.0, size.1);
}

/// Render off-screen into an EGL pbuffer and dump the frame as a PPM image.
///
/// Enabled by setting `VRM_VIEWER_HEADLESS_PPM=<out.ppm>`; useful to verify
/// rendering without a window or compositor (and for CI smoke tests).
fn headless_render(path: &str, out_ppm: &str) {
    let mut model =
        vrm_engine::load_glb_from_path(path).unwrap_or_else(|e| panic!("failed to load {path}: {e}"));

    let event_loop = EventLoop::new().expect("event loop");
    let raw_display = event_loop.display_handle().unwrap().as_raw();
    let gl_display = unsafe { Display::new(raw_display, DisplayApiPreference::Egl) }
        .expect("gl display");

    let config_template = ConfigTemplateBuilder::new()
        .with_depth_size(24)
        .with_alpha_size(8)
        .with_surface_type(ConfigSurfaceTypes::PBUFFER);
    let config = unsafe {
        gl_display
            .find_configs(config_template.build())
            .expect("no configs")
            .find(|c| c.depth_size() > 0)
            .expect("no config with depth buffer")
    };

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(glutin::context::Version::new(3, 3))))
        .build(None);
    let context = unsafe {
        gl_display
            .create_context(&config, &context_attributes)
            .expect("context")
    };

    let (width, height) = (1280u32, 720u32);
    let surface_attributes = SurfaceAttributesBuilder::<PbufferSurface>::new().build(
        std::num::NonZeroU32::new(width).unwrap(),
        std::num::NonZeroU32::new(height).unwrap(),
    );
    let surface = unsafe {
        gl_display
            .create_pbuffer_surface(&config, &surface_attributes)
            .expect("pbuffer surface")
    };
    let _context = context.make_current(&surface).expect("make current");

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let c = std::ffi::CString::new(s).unwrap();
            gl_display.get_proc_address(&c)
        })
    };
    let mut renderer = Renderer::new(std::sync::Arc::new(gl), &model.vrm.doc, &model.images, &model.vrm.material_properties);
    let mut view_model = model::extract(&model);
    let mut camera = Camera::default();
    camera.frame(view_model.aabb_min, view_model.aabb_max);
    if let (Ok(y), Ok(p), Ok(d)) = (
        std::env::var("VRM_VIEWER_YAW"),
        std::env::var("VRM_VIEWER_PITCH"),
        std::env::var("VRM_VIEWER_DIST"),
    ) {
        camera.yaw = y.parse().unwrap();
        camera.pitch = p.parse().unwrap();
        camera.dist = d.parse().unwrap();
    }

    // Simulate a few frames so spring bones settle into a pose.
    if let Ok(expr) = std::env::var("VRM_VIEWER_EXPRESSION") {
        if let Some(id) = ExpressionId::preset(&expr) {
            model.vrm.set_expression(&id, 1.0);
            model.vrm.apply_expressions();
            println!("expression {expr} applied");
            for e in model.vrm.expressions.expressions() {
                if e.id == id {
                    println!(
                        "  expr {} binds={} override={:?} is_binary={}",
                        e.id,
                        e.morph_binds.len(),
                        e.override_blink,
                        e.is_binary,
                    );
                    for b in &e.morph_binds {
                        println!("    bind node={} morph={} weight={}", b.node, b.index, b.weight);
                    }
                }
            }
            println!(
                "  nodes_with_weights:"
            );
            for n in 0..model.vrm.node_count() {
                let w = model.vrm.morph_weights(n);
                if let Some(w) = w {
                    if w.iter().any(|&x| x != 0.0) {
                        println!("  node {n} weights={w:?}");
                    }
                }
            }
            println!("  view model meshes (node, morphs):");
            for m in &view_model.meshes {
                println!(
                    "    node={} morphs={}",
                    m.node,
                    m.morph_delta_pos.len(),
                );
            }
        } else {
            eprintln!("unknown expression preset: {expr}");
        }
    }
    for _ in 0..30 {
        step_and_render(
            &mut model,
            &mut view_model,
            &camera,
            &mut renderer,
            (width, height),
            1.0 / 60.0,
        );
    }

    let gl = renderer.gl();
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe {
        gl.read_buffer(glow::FRONT);
        gl.read_pixels(
            0,
            0,
            width as i32,
            height as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
    }
    write_ppm(out_ppm, width, height, &pixels);
    println!("headless render written to {out_ppm}");
}

fn write_ppm(path: &str, width: u32, height: u32, rgba: &[u8]) {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    // GL's origin is bottom-left; flip vertically for the image.
    for y in (0..height).rev() {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i], rgba[i + 1], rgba[i + 2]]);
        }
    }
    std::fs::write(path, out).expect("write ppm");
}

struct App {
    model: Option<LoadedModel>,
    viewer: Option<Viewer>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.viewer.is_none() {
            let model = self.model.take().expect("model set before run");
            self.viewer = Some(Viewer::new(event_loop, model));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        let egui_consumed = viewer
            .egui_state
            .on_window_event(&viewer.window, &event)
            .consumed;
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                viewer.size = (size.width.max(1), size.height.max(1));
                if let (Some(w), Some(h)) = (
                    std::num::NonZeroU32::new(size.width),
                    std::num::NonZeroU32::new(size.height),
                ) {
                    viewer.surface.resize(&viewer._context, w, h);
                }
            }
            WindowEvent::RedrawRequested => viewer.render_frame(),
            WindowEvent::MouseInput { state, button, .. } if !egui_consumed => {
                viewer.dragging = if state == ElementState::Pressed {
                    Some(button)
                } else {
                    None
                };
            }
            WindowEvent::CursorMoved { position, .. } if !egui_consumed => {
                let dx = position.x - viewer.last_cursor.0;
                let dy = position.y - viewer.last_cursor.1;
                viewer.last_cursor = (position.x, position.y);
                match viewer.dragging {
                    Some(MouseButton::Left) => {
                        viewer.camera.yaw += dx as f32 * 0.01;
                        viewer.camera.pitch =
                            (viewer.camera.pitch + dy as f32 * 0.01).clamp(-1.5, 1.5);
                    }
                    Some(MouseButton::Right) => {
                        let s = viewer.camera.dist * 0.002;
                        viewer.camera.target.x -= dx as f32 * s;
                        viewer.camera.target.y += dy as f32 * s;
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !egui_consumed => match delta {
                MouseScrollDelta::LineDelta(_, y) => {
                    viewer.camera.dist =
                        (viewer.camera.dist * (1.0 - y * 0.1)).clamp(0.2, 50.0);
                }
                MouseScrollDelta::PixelDelta(p) => {
                    viewer.camera.dist =
                        (viewer.camera.dist * (1.0 - p.y as f32 * 0.001)).clamp(0.2, 50.0);
                }
            },
            // Keyboard shortcuts must always work: egui consumes keys whenever
            // a widget has keyboard focus (e.g. after clicking a slider), but
            // that must not disable `1`..`=`/`r`.
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Character(ch) => {
                        use ExpressionPreset::*;
                        let preset = match ch.as_str() {
                            "1" => Some(Happy),
                            "2" => Some(Angry),
                            "3" => Some(Sad),
                            "4" => Some(Surprised),
                            "5" => Some(Relaxed),
                            "6" => Some(Blink),
                            "7" => Some(Neutral),
                            "8" => Some(Aa),
                            "9" => Some(Ih),
                            "0" => Some(Ou),
                            "-" => Some(Oh),
                            "=" => Some(Ee),
                            _ => None,
                        };
                        if let Some(preset) = preset {
                            viewer.apply_expression(preset, 1.0);
                        } else if ch == "r" {
                            viewer.model.vrm.reset_pose();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(viewer) = &self.viewer {
            viewer.window.request_redraw();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "fixtures/AvatarSample_A.vrm".to_string());

    let model = vrm_engine::load_glb_from_path(&path)
        .unwrap_or_else(|e| panic!("failed to load {path}: {e}"));

    // Optional off-screen render (no window): `VRM_VIEWER_HEADLESS_PPM=out.ppm`.
    if let Ok(out) = std::env::var("VRM_VIEWER_HEADLESS_PPM") {
        drop(model);
        headless_render(&path, &out);
        return;
    }

    println!(
        "loaded {} (VRM {}): {} nodes, {} meshes, {} spring groups",
        model_path(&model),
        model.vrm.version,
        model.vrm.node_count(),
        model.vrm.doc.meshes().len(),
        model.vrm.spring_bones.groups.len(),
    );
    println!(
        "controls: LMB orbit, RMB pan, wheel zoom, 1-0/-= expressions, r reset, Esc quit"
    );

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        model: Some(model),
        viewer: None,
    };
    event_loop.run_app(&mut app).expect("run app");
}
