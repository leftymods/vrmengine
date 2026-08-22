//! Drag-and-drop VRM viewer built on `bevy_vrm` (which pulls in the
//! `bevy_shader_mtoon` WGSL shader unchanged) + `bevy_egui` for the control
//! window and `bevy_panorbit_camera` for the orbit camera. The engine side is
//! Bevy 0.19 / wgpu (Vulkan); there is no custom OpenGL renderer anymore.

use std::f32::consts::PI;
use std::sync::OnceLock;

use bevy::asset::AssetMetaCheck;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, EguiContexts};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use bevy_vrm::{
    mtoon::MtoonSun,
    VrmInstance,
    VrmPlugins,
};

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
        .add_systems(Update, (load_model, read_dropped_files))
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
        Transform::from_xyz(0.0, 1.3, 3.0),
        PanOrbitCamera {
            focus: Vec3::new(0.0, 0.9, 0.0),
            // Raw mouse motion / wheel ticks are multiplied straight by
            // these; keep them low so a full orbit needs a wide drag. A
            // whole-window drag at 0.04 sweeps ~14 degrees.
            orbit_sensitivity: 0.04,
            pan_sensitivity: 0.05,
            zoom_sensitivity: 0.1,
            // Less input smoothing so the camera tracks the hand directly
            // instead of gliding past the cursor stop point.
            orbit_smoothness: 0.4,
            pan_smoothness: 0.3,
            zoom_smoothness: 0.4,
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

fn update_ui(mut contexts: EguiContexts, mut settings: ResMut<Settings>) {
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
            ui.separator();
            ui.label(build_label());
        });
}