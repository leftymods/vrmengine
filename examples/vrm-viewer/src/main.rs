//! Drag-and-drop VRM viewer built on `bevy_vrm` (which pulls in the
//! `bevy_shader_mtoon` WGSL shader unchanged) + `bevy_egui` for the control
//! window and `bevy_panorbit_camera` for the orbit camera. The engine side is
//! Bevy 0.19 / wgpu (Vulkan); there is no custom OpenGL renderer anymore.

use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::OnceLock;

use bevy::asset::AssetMetaCheck;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use bevy_panorbit_camera::{EguiWantsFocus, PanOrbitCamera, PanOrbitCameraPlugin};
use bevy_vrm::{
    mtoon::MtoonSun,
    VrmInstance,
    VrmPlugins,
};

mod expressions;

/// Build label baked by `build.rs` (`VRM_VIEWER_BUILD_HASH` + `..._BUILD_TIME`
/// as `HH:MM:SS`). Shown in the egui window so the freshness of the binary is
/// visible at a glance.
static BUILD_LABEL: OnceLock<String> = OnceLock::new();

fn build_label() -> &'static str {
    BUILD_LABEL.get_or_init(|| {
        let hash = option_env!("VRM_VIEWER_BUILD_HASH").unwrap_or("unknown");
        let secs: u64 = option_env!("VRM_VIEWER_BUILD_TIME")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let time = if secs > 0 {
            let h = (secs / 3_600) % 24;
            let m = (secs / 60) % 60;
            let s = secs % 60;
            format!("{h:02}:{m:02}:{s:02}")
        } else {
            "??:??:??".to_string()
        };
        format!("vrm-viewer {hash} build {time}")
    })
}

fn main() {
    // Model source, most specific first:
    //   1. CLI argument (absolute or relative filesystem path)
    //   2. The bundled fixture, resolved from the package dir so the binary
    //      works no matter which directory it is launched from.
    let default_model = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/AvatarSample_A.vrm"
    );
    let model = std::env::args().nth(1).unwrap_or_else(|| default_model.to_string());

    App::new()
        .insert_resource(ClearColor(Color::linear_rgb(0.12, 0.14, 0.17)))
        .insert_resource(Settings {
            model,
            loaded: String::new(),
        })
        .init_resource::<expressions::ExpressionRig>()
        .init_resource::<AutoRotate>()
        // NOTE: no EguiFocusIncludesHover here. Orbit is bound to the middle
        // mouse button, so left-clicks on UI can never move the camera; a
        // hover-based block would only swallow legit MMB drags that start
        // near the always-open egui windows.
        .add_plugins((
            // Model paths are absolute (CLI arg / dropped file / baked
            // fixture path); Bevy denies loading files outside the asset root
            // unless explicitly allowed.
            DefaultPlugins.set(AssetPlugin {
                meta_check: AssetMetaCheck::Never,
                unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow,
                ..default()
            }),
            EguiPlugin::default(),
            PanOrbitCameraPlugin,
            VrmPlugins,
        ))
        .add_systems(Startup, setup)
        .add_systems(EguiPrimaryContextPass, update_ui)
        .add_systems(
            Update,
            (
                load_model,
                read_dropped_files,
                auto_rotate,
                wasd_move,
                expressions::collect_rig,
                expressions::apply_expressions,
            ),
        )
        // `bevy_world_serialization` spawns VRM scenes through the type
        // registry; with a minimal bevy feature set nothing registers the
        // standard scene components, and spawn panics on the first missing
        // one. Register them explicitly.
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<Name>()
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .run();
}

#[derive(Resource)]
struct Settings {
    /// Filesystem/asset path of the VRM to display.
    model: String,
    /// The path that has actually been spawned, so we only reload on change.
    loaded: String,
}

fn setup(mut commands: Commands) {
    // Orbit camera framing the avatar head/chest. We spawn the Camera3d
    // ourselves so we can trim render cost for the software Vulkan pipeline
    // (llvmpipe): MSAA off and no filmic tonemapping pass - MToon output is
    // already in display range, like VRoid Studio.
    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Tonemapping::None,
        // Skip the deband fullscreen pass; every fragment counts on the
        // software Vulkan pipeline and the sliders need all the FPS they can
        // get for responsive feedback.
        DebandDither::Disabled,
        Transform::from_xyz(0.0, 1.3, 3.0),
        PanOrbitCamera {
            focus: Vec3::new(0.0, 0.9, 0.0),
            // Mouse-look controls: right-drag orbits, middle-drag pans,
            // wheel zooms. No modifiers - every button works as a plain
            // press so the scheme survives odd X11 button mappings. Left
            // stays free for the UI and never moves the camera.
            button_orbit: bevy::input::mouse::MouseButton::Right,
            button_pan: bevy::input::mouse::MouseButton::Middle,
            // A full-window MMB drag sweeps ~216 degrees, close to Blender's
            // viewport feel; smoothing kept low so the camera tracks 1:1.
            orbit_sensitivity: 0.6,
            pan_sensitivity: 0.6,
            zoom_sensitivity: 0.4,
            orbit_smoothness: 0.25,
            pan_smoothness: 0.2,
            zoom_smoothness: 0.25,
            ..default()
        },
    ));

    // Single directional light driving the MToon shading (`MtoonSun` is the
    // marker `bevy_shader_mtoon` looks for to feed `light_dir`/`light_color`).
    // Shadow maps stay off: on the software Vulkan pipeline (llvmpipe) they
    // dominate frame time, and MToon's shade band does not depend on them.
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            -PI / 4.0,
            -PI / 3.0,
        )),
        MtoonSun,
    ));
}

fn load_model(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut settings: ResMut<Settings>,
    vrms: Query<Entity, With<VrmInstance>>,
) {
    if settings.model == settings.loaded {
        return;
    }
    // Despawn the previous avatar (single live instance at a time).
    for entity in vrms.iter() {
        commands.entity(entity).despawn();
    }
    let mut transform = Transform::default();
    // VRM avatars face -Z by convention; turn them toward the camera.
    transform.rotate_y(PI);
    commands.spawn((transform, VrmInstance(asset_server.load(settings.model.clone()))));
    settings.loaded = settings.model.clone();
}

fn read_dropped_files(mut events: MessageReader<FileDragAndDrop>, mut settings: ResMut<Settings>) {
    for event in events.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            let path = path_buf.to_string_lossy().to_string();
            info!("DroppedFile: {path}");
            settings.model = path;
        }
    }
}

fn update_ui(
    mut contexts: EguiContexts,
    mut settings: ResMut<Settings>,
    mut rig: ResMut<expressions::ExpressionRig>,
    mut auto: ResMut<AutoRotate>,
    mut panorbit: Query<&mut PanOrbitCamera>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    bevy_egui::egui::Window::new("VRM Viewer")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Model path:");
                ui.text_edit_singleline(&mut settings.model);
                if ui.button("Load").clicked() {
                    // `settings.model` is already mutated by the text edit;
                    // `load_model` will pick up the change next frame.
                }
            });
            ui.label("Drop a .vrm file into the window to load it.");
            ui.label(
                "Controls: RMB drag = orbit, MMB drag = pan, wheel = zoom, \
                 WASD = move, E/Q = up/down.",
            );

            // Camera view presets + turntable.
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("View:");
                for (idx, (label, ..)) in VIEW_PRESETS.iter().enumerate() {
                    if ui.button(*label).clicked()
                        && let Ok(mut cam) = panorbit.single_mut()
                    {
                        apply_preset(&mut cam, idx);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut auto.0, "Auto-rotate");
                if ui.button("Reset view").clicked()
                    && let Ok(mut cam) = panorbit.single_mut()
                {
                    apply_preset(&mut cam, PRESET_HOME);
                }
            });

            ui.separator();
            ui.label(build_label());
        });

    expressions_window(ctx, &mut rig);
}

/// Camera presets: `(label, yaw, pitch, radius, focus)`. The avatar faces
/// +Z after the spawn-time Y flip, so yaw 0 is a frontal view.
const VIEW_PRESETS: &[(&str, f32, f32, f32, [f32; 3])] = &[
    ("Face", 0.0, -0.05, 0.9, [0.0, 1.45, 0.0]),
    ("Upper", 0.0, -0.1, 1.9, [0.0, 1.2, 0.0]),
    ("Full", 0.0, -0.15, 3.4, [0.0, 0.85, 0.0]),
    ("Front", 0.0, -0.1, 2.8, [0.0, 1.05, 0.0]),
    ("Back", PI, -0.1, 2.8, [0.0, 1.05, 0.0]),
    ("Left", FRAC_PI_2, -0.1, 2.8, [0.0, 1.05, 0.0]),
    ("Right", -FRAC_PI_2, -0.1, 2.8, [0.0, 1.05, 0.0]),
    ("Top", 0.0, -1.35, 3.0, [0.0, 0.95, 0.0]),
];

// Default preset used by the "Reset view" button.
const PRESET_HOME: usize = 2;

fn apply_preset(cam: &mut PanOrbitCamera, idx: usize) {
    let (_, yaw, pitch, radius, focus) = VIEW_PRESETS[idx];
    cam.target_yaw = yaw;
    cam.target_pitch = pitch;
    cam.target_radius = radius;
    cam.focus = Vec3::from_array(focus);
}

/// Fly-style movement: WASD moves the orbit rig across the ground plane
/// relative to where the camera looks, E/Q move straight up/down. Speed
/// scales with zoom distance so it feels identical at any radius.
/// Ignored while egui wants the keyboard (typing in the path field).
fn wasd_move(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    egui_focus: Res<EguiWantsFocus>,
    mut q: Query<&mut PanOrbitCamera>,
) {
    if egui_focus.prev || egui_focus.curr {
        return;
    }
    for mut cam in &mut q {
        let rot = Quat::from_axis_angle(Vec3::Y, cam.target_yaw);
        let forward = rot * Vec3::NEG_Z;
        let right = rot * Vec3::X;

        let mut dir = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) {
            dir += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            dir -= forward;
        }
        if keys.pressed(KeyCode::KeyD) {
            dir += right;
        }
        if keys.pressed(KeyCode::KeyA) {
            dir -= right;
        }
        if keys.pressed(KeyCode::KeyE) {
            dir += Vec3::Y;
        }
        if keys.pressed(KeyCode::KeyQ) {
            dir -= Vec3::Y;
        }
        if dir == Vec3::ZERO {
            continue;
        }
        let speed = cam.target_radius.max(0.5);
        cam.target_focus += dir.normalize_or_zero() * speed * time.delta_secs();
    }
}

#[derive(Resource, Default)]
struct AutoRotate(bool);

/// Slow turntable spin while enabled; pauses while the user holds an orbit /
/// pan button so the spin never fights a manual drag.
fn auto_rotate(
    time: Res<Time>,
    auto: Res<AutoRotate>,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    mut q: Query<&mut PanOrbitCamera>,
) {
    use bevy::input::mouse::MouseButton;
    if !auto.0 || mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) {
        return;
    }
    for mut cam in &mut q {
        cam.target_yaw += time.delta_secs() * 0.5;
    }
}

/// Expression manager: sliders for every VRM blendshape group plus quick
/// preset buttons and a reset.
fn expressions_window(ctx: &mut bevy_egui::egui::Context, rig: &mut expressions::ExpressionRig) {
    bevy_egui::egui::Window::new("Expressions")
        .default_width(280.0)
        .show(ctx, |ui| {
            if !rig.parse_done() {
                ui.label("waiting for model…");
                return;
            }
            if rig.groups.is_empty() {
                ui.label("no blend shapes in this model");
                return;
            }

            bevy_egui::egui::Grid::new("expr_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for i in 0..rig.values.len() {
                        ui.label(rig.groups[i].name.clone());
                        if rig.groups[i].is_binary {
                            let mut on = rig.values[i] > 0.5;
                            if ui.checkbox(&mut on, "").changed() {
                                rig.values[i] = if on { 1.0 } else { 0.0 };
                            }
                        } else {
                            ui.add(bevy_egui::egui::Slider::new(
                                &mut rig.values[i],
                                0.0..=1.0,
                            )
                            .show_value(true));
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                // One-click presets for the standard emotion groups.
                for preset in ["joy", "angry", "sorrow", "fun", "blink"] {
                    if ui.button(preset).clicked() {
                        for (i, g) in rig.groups.iter().enumerate() {
                            if g.name.eq_ignore_ascii_case(preset) {
                                rig.values[i] = 1.0;
                            }
                        }
                    }
                }
                if ui.button("Reset all").clicked() {
                    rig.values.iter_mut().for_each(|v| *v = 0.0);
                }
            });
        });
}