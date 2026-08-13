use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use egui_winit::winit;
use winit::raw_window_handle::HasWindowHandle as _;

use crate::{load_vrm, VRM};

pub use crate::log::{debug, error, info, warn};

struct GlutinWindowContext {
    window: winit::window::Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_display: glutin::display::Display,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

impl GlutinWindowContext {
    unsafe fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
        use glutin::context::NotCurrentGlContext as _;
        use glutin::display::GetGlDisplay as _;
        use glutin::display::GlDisplay as _;
        use glutin::prelude::GlSurface as _;

        let winit_window_builder = winit::window::WindowAttributes::default()
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize {
                width: 800.0,
                height: 600.0,
            })
            .with_title("VRM Engine");

        let config_template_builder = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        let (mut window, gl_config) = glutin_winit::DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(winit_window_builder.clone()))
            .build(
                event_loop,
                config_template_builder,
                |mut config_iterator| {
                    config_iterator.next().expect(
                        "failed to find a matching configuration for creating glutin config",
                    )
                },
            )
            .expect("failed to create gl_config");

        let gl_display = gl_config.display();

        let raw_window_handle = window.as_ref().map(|w| {
            w.window_handle()
                .expect("failed to get window handle")
                .as_raw()
        });

        let context_attributes =
            glutin::context::ContextAttributesBuilder::new().build(raw_window_handle);
        let fallback_context_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);

        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_config
                        .display()
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context even with fallback attributes")
                })
        };

        let window = window.take().unwrap_or_else(|| {
            glutin_winit::finalize_window(event_loop, winit_window_builder.clone(), &gl_config)
                .expect("failed to finalize glutin window")
        });

        let (width, height): (u32, u32) = window.inner_size().into();
        let width = NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN);
        let height = NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN);
        let surface_attributes = glutin::surface::SurfaceAttributesBuilder::<
            glutin::surface::WindowSurface,
        >::new()
        .build(
            window
                .window_handle()
                .expect("failed to get window handle")
                .as_raw(),
            width,
            height,
        );

        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .unwrap()
        };

        let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

        gl_surface
            .set_swap_interval(&gl_context, glutin::surface::SwapInterval::DontWait)
            .unwrap();

        Self {
            window,
            gl_context,
            gl_display,
            gl_surface,
        }
    }

    fn window(&self) -> &winit::window::Window {
        &self.window
    }

    fn resize(&self, physical_size: winit::dpi::PhysicalSize<u32>) {
        use glutin::surface::GlSurface as _;
        self.gl_surface.resize(
            &self.gl_context,
            physical_size.width.try_into().unwrap(),
            physical_size.height.try_into().unwrap(),
        );
    }

    fn swap_buffers(&self) -> glutin::error::Result<()> {
        use glutin::surface::GlSurface as _;
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    fn get_proc_address(&self, addr: &std::ffi::CStr) -> *const std::ffi::c_void {
        use glutin::display::GlDisplay as _;
        self.gl_display.get_proc_address(addr)
    }
}

#[derive(Debug)]
enum UserEvent {
    Redraw(std::time::Duration),
}

struct App {
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    resume_count: u32,
    gl_window: Option<GlutinWindowContext>,
    gl: Option<Arc<glow::Context>>,
    egui_glow: Option<egui_glow::EguiGlow>,
    renderer: Option<crate::renderer::Renderer>,
    repaint_delay: std::time::Duration,
    clear_color: [f32; 3],
    last_frame_time: Instant,
    frame_count: u64,
    last_fps_log: Instant,
    vrm_path: String,
    vrm: Option<VRM>,
    load_error: Option<String>,
}

impl Drop for App {
    fn drop(&mut self) {
        info("App dropped (event loop ended)");
    }
}

impl App {
    fn new(proxy: winit::event_loop::EventLoopProxy<UserEvent>) -> Self {
        Self {
            proxy,
            resume_count: 0,
            gl_window: None,
            gl: None,
            egui_glow: None,
            renderer: None,
            repaint_delay: std::time::Duration::MAX,
            clear_color: [0.1, 0.1, 0.1],
            last_frame_time: Instant::now(),
            frame_count: 0,
            last_fps_log: Instant::now(),
            vrm_path: String::new(),
            vrm: None,
            load_error: None,
        }
    }

    fn try_load_vrm(&mut self, path: &str) {
        info(&format!("Loading VRM from file: {}", path));
        self.load_error = None;
        match std::fs::read(path) {
            Ok(bytes) => {
                debug(&format!("Read {} bytes from file", bytes.len()));
                match load_vrm(&bytes) {
                    Ok(loaded) => {
                        info(&format!("VRM parsed successfully: {}", path));
                        if let Some(renderer) = &mut self.renderer {
                            info("Uploading scene to GPU...");
                            renderer.upload_scene(self.gl.as_ref().unwrap(), &loaded.scene);
                            info("Scene uploaded to GPU");
                        }
                        self.vrm = Some(loaded);
                        info("VRM model set as current");
                    }
                    Err(e) => {
                        self.load_error = Some(format!("Load error: {}", e));
                        error(self.load_error.as_ref().unwrap());
                    }
                }
            }
            Err(e) => {
                self.load_error = Some(format!("IO error: {}", e));
                error(self.load_error.as_ref().unwrap());
            }
        }
    }
}

impl winit::application::ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.resume_count += 1;
        info(&format!("resumed() called ({})", self.resume_count));
        info("Creating GL display and context...");
        let (gl_window, gl) = create_display(event_loop);
        let gl = Arc::new(gl);
        info("GL context created");
        unsafe {
            use glow::HasContext as _;
            info(&format!(
                "GL_VENDOR: {}",
                gl.get_parameter_string(glow::VENDOR)
            ));
            info(&format!(
                "GL_RENDERER: {}",
                gl.get_parameter_string(glow::RENDERER)
            ));
            info(&format!(
                "GL_VERSION: {}",
                gl.get_parameter_string(glow::VERSION)
            ));
        }
        gl_window.window().set_visible(true);
        info("Window visible");

        debug("Initializing egui_glow...");
        let egui_glow =
            egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, true);
        info("egui_glow initialized");

        let event_loop_proxy = egui::mutex::Mutex::new(self.proxy.clone());
        egui_glow
            .egui_ctx
            .set_request_repaint_callback(move |info| {
                event_loop_proxy
                    .lock()
                    .send_event(UserEvent::Redraw(info.delay))
                    .expect("Cannot send event");
            });
        debug("Repaint callback installed");

        debug("Creating renderer...");
        let renderer = crate::renderer::Renderer::new(&gl);
        info("Renderer initialized");
        self.gl_window = Some(gl_window);
        self.gl = Some(gl);
        self.egui_glow = Some(egui_glow);
        self.renderer = Some(renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;
        debug(&format!("WindowEvent: {}", window_event_name(&event)));
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            info("Window close requested, exiting");
            event_loop.exit();
            return;
        }

        if let WindowEvent::Resized(physical_size) = &event {
            debug(&format!(
                "Window resized to {}x{}",
                physical_size.width, physical_size.height
            ));
            self.gl_window.as_mut().unwrap().resize(*physical_size);
        }

        let event_response = self
            .egui_glow
            .as_mut()
            .unwrap()
            .on_window_event(self.gl_window.as_mut().unwrap().window(), &event);

        if let WindowEvent::DroppedFile(path) = &event {
            self.vrm_path = path.to_string_lossy().to_string();
            self.load_error = None;
            let path = path.clone();
            info(&format!("File dropped: {}", path.display()));
            match std::fs::read(&path) {
                Ok(bytes) => {
                    debug(&format!("Read {} bytes from dropped file", bytes.len()));
                    match load_vrm(&bytes) {
                        Ok(loaded) => {
                            info(&format!("Loaded VRM: {}", path.display()));
                            if let Some(renderer) = &mut self.renderer {
                                renderer.upload_scene(self.gl.as_ref().unwrap(), &loaded.scene);
                            }
                            self.vrm = Some(loaded);
                        }
                        Err(e) => {
                            self.load_error = Some(format!("Load error: {}", e));
                            error(self.load_error.as_ref().unwrap());
                        }
                    }
                }
                Err(e) => {
                    self.load_error = Some(format!("IO error: {}", e));
                    error(self.load_error.as_ref().unwrap());
                }
            }
        }

        if matches!(event, WindowEvent::RedrawRequested) {
            debug("RedrawRequested: frame start");
            let now = Instant::now();
            let delta = now.duration_since(self.last_frame_time).as_secs_f32();
            self.last_frame_time = now;
            self.frame_count += 1;
            if now.duration_since(self.last_fps_log).as_secs_f32() >= 1.0 {
                debug(&format!("Frame #{}, FPS: {:.1}", self.frame_count, 1.0 / delta.max(1e-6)));
                self.last_fps_log = now;
            }

            if let Some(vrm) = &mut self.vrm {
                vrm.update(delta);
            }
            debug("frame: updating vrm done");

            let size = self.gl_window.as_ref().unwrap().window().inner_size();
            let gl = self.gl.as_mut().unwrap();
            debug("frame: got size and gl");
            unsafe {
                use glow::HasContext as _;
                gl.clear_color(
                    self.clear_color[0],
                    self.clear_color[1],
                    self.clear_color[2],
                    1.0,
                );
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            }
            debug("frame: cleared");

            if let Some(renderer) = &mut self.renderer {
                if let Some(vrm) = &mut self.vrm {
                    renderer.draw(gl, &mut vrm.scene, size.width, size.height);
                }
            }
            debug("frame: renderer.draw done");

            let mut quit = false;
            let mut load_request: Option<String> = None;
            let mut reset_expressions = false;
            let vrm_path = &mut self.vrm_path;
            let load_error = &mut self.load_error;
            let vrm = &mut self.vrm;

            debug("frame: running egui UI");
            self.egui_glow
                .as_mut()
                .unwrap()
                .run(self.gl_window.as_mut().unwrap().window(), |ui| {
                    egui::Panel::left("side_panel").show(ui, |ui| {
                        ui.heading("VRM Engine");
                        ui.horizontal(|ui| {
                            ui.label("Path:");
                            ui.text_edit_singleline(vrm_path);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("VRM / GLB", &["vrm", "glb"])
                                    .pick_file()
                                {
                                    *vrm_path = path.to_string_lossy().to_string();
                                }
                            }
                        });
                        if ui.button("Load VRM").clicked() {
                            *load_error = None;
                            load_request = Some(vrm_path.clone());
                        }
                        if let Some(err) = load_error {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                        if let Some(v) = vrm {
                            ui.separator();
                            if let Some(meta) = v.meta() {
                                ui.label(format!(
                                    "Title: {}",
                                    meta.title().unwrap_or("Unknown")
                                ));
                                if let Some(ver) = meta.version() {
                                    ui.label(format!("Version: {}", ver));
                                }
                                let authors = meta.authors();
                                if !authors.is_empty() {
                                    ui.label(format!("Authors: {}", authors.join(", ")));
                                }
                            }
                            ui.heading("Expressions");
                            if let Some(em) = v.expressions_mut() {
                                let names: Vec<String> =
                                    em.expressions.iter().map(|e| e.name.clone()).collect();
                                for name in &names {
                                    let mut value = em.get_value(name);
                                    ui.horizontal(|ui| {
                                        ui.label(name);
                                        if ui
                                            .add(egui::Slider::new(&mut value, 0.0..=1.0))
                                            .changed()
                                        {
                                            let _ = em.set_value(name, value);
                                        }
                                    });
                                }
                                if ui.button("Reset All").clicked() {
                                    reset_expressions = true;
                                }
                            }
                        }
                        if ui.button("Quit").clicked() {
                            quit = true;
                        }
                    });
                });

            if let Some(path) = load_request {
                self.try_load_vrm(&path);
            }
            if reset_expressions {
                if let Some(v) = &mut self.vrm {
                    if let Some(em) = v.expressions_mut() {
                        for expr in &mut em.expressions {
                            expr.weight = 0.0;
                        }
                    }
                }
            }

            if quit {
                event_loop.exit();
            } else {
                event_loop.set_control_flow(if self.repaint_delay.is_zero() {
                    self.gl_window.as_mut().unwrap().window().request_redraw();
                    winit::event_loop::ControlFlow::Poll
                } else if let Some(repaint_after_instant) =
                    std::time::Instant::now().checked_add(self.repaint_delay)
                {
                    winit::event_loop::ControlFlow::WaitUntil(repaint_after_instant)
                } else {
                    winit::event_loop::ControlFlow::Wait
                });
            }

            debug("frame: egui UI done, control flow set");
            debug("frame: painting egui");
            self.egui_glow
                .as_mut()
                .unwrap()
                .paint(self.gl_window.as_mut().unwrap().window());
            debug("frame: painted, swapping buffers");
            self.gl_window.as_mut().unwrap().swap_buffers().unwrap();
            debug("frame: buffers swapped, frame complete");
        }

        if event_response.repaint {
            self.gl_window.as_mut().unwrap().window().request_redraw();
        }
    }

    fn user_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        event: UserEvent,
    ) {
        match event {
            UserEvent::Redraw(delay) => self.repaint_delay = delay,
        }
    }

    fn new_events(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        match &cause {
            winit::event::StartCause::Init => debug("NewEvents: Init"),
            winit::event::StartCause::ResumeTimeReached { .. } => {
                debug("NewEvents: ResumeTimeReached -> request_redraw");
                self.gl_window.as_mut().unwrap().window().request_redraw();
            }
            winit::event::StartCause::WaitCancelled { .. } => debug("NewEvents: WaitCancelled"),
            winit::event::StartCause::Poll => debug("NewEvents: Poll"),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        debug("about_to_wait");
    }

    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        info("exiting() called");
        if let Some(egui) = self.egui_glow.as_mut() {
            egui.destroy();
        }
    }
}

fn window_event_name(event: &winit::event::WindowEvent) -> &'static str {
    use winit::event::WindowEvent;
    match event {
        WindowEvent::ActivationTokenDone { .. } => "ActivationTokenDone",
        WindowEvent::Resized(_) => "Resized",
        WindowEvent::Moved(_) => "Moved",
        WindowEvent::CloseRequested => "CloseRequested",
        WindowEvent::Destroyed => "Destroyed",
        WindowEvent::DroppedFile(_) => "DroppedFile",
        WindowEvent::HoveredFile(_) => "HoveredFile",
        WindowEvent::HoveredFileCancelled => "HoveredFileCancelled",
        WindowEvent::Focused(_) => "Focused",
        WindowEvent::KeyboardInput { .. } => "KeyboardInput",
        WindowEvent::ModifiersChanged(_) => "ModifiersChanged",
        WindowEvent::Ime(_) => "Ime",
        WindowEvent::CursorMoved { .. } => "CursorMoved",
        WindowEvent::CursorEntered { .. } => "CursorEntered",
        WindowEvent::CursorLeft { .. } => "CursorLeft",
        WindowEvent::MouseWheel { .. } => "MouseWheel",
        WindowEvent::MouseInput { .. } => "MouseInput",
        WindowEvent::PinchGesture { .. } => "PinchGesture",
        WindowEvent::PanGesture { .. } => "PanGesture",
        WindowEvent::DoubleTapGesture { .. } => "DoubleTapGesture",
        WindowEvent::RotationGesture { .. } => "RotationGesture",
        WindowEvent::Touch(_) => "Touch",
        WindowEvent::TouchpadPressure { .. } => "TouchpadPressure",
        WindowEvent::AxisMotion { .. } => "AxisMotion",
        WindowEvent::Occluded(_) => "Occluded",
        WindowEvent::RedrawRequested => "RedrawRequested",
        WindowEvent::ThemeChanged(_) => "ThemeChanged",
        WindowEvent::ScaleFactorChanged { .. } => "ScaleFactorChanged",
    }
}

fn create_display(
    event_loop: &winit::event_loop::ActiveEventLoop,
) -> (GlutinWindowContext, glow::Context) {
    let glutin_window_context = unsafe { GlutinWindowContext::new(event_loop) };
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let s = CString::new(s)
                .expect("failed to construct C string from string for gl proc address");
            glutin_window_context.get_proc_address(&s)
        })
    };
    gl::load_with(|symbol| {
        let s = CString::new(symbol).unwrap();
        glutin_window_context.get_proc_address(&s) as *const _
    });
    (glutin_window_context, gl)
}

pub fn run() -> anyhow::Result<()> {
    info("Starting VRM Engine viewer");
    let event_loop = winit::event_loop::EventLoop::<UserEvent>::with_user_event()
        .build()?;
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    info("Event loop created, running");
    let result = event_loop.run_app(&mut app);
    match &result {
        Ok(()) => info("Event loop exited normally"),
        Err(e) => error(&format!("Event loop error: {e:?}")),
    }
    result?;
    Ok(())
}
