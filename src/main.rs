fn main() {
    let mesa_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()));
    let force_llvmpipe = mesa_dir
        .as_ref()
        .map(|d| d.join("libgallium_wgl.dll").exists())
        .unwrap_or(false);
    if force_llvmpipe {
        // Mesa's D3D12 WGL driver can crash hard on swap_buffers. Its software
        // renderer (llvmpipe) is stable and still provides a full OpenGL core profile.
        std::env::set_var("GALLIUM_DRIVER", "llvmpipe");
    }
    vrmengine::log::init();
    if force_llvmpipe {
        vrmengine::log::info("Mesa DLL found next to exe -> forcing software renderer (llvmpipe)");
    }
    if let Err(e) = vrmengine::viewer::run() {
        vrmengine::log::error(&format!("Fatal error: {e}"));
    }
}
