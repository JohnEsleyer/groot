use winit::window::Window;

/// Attach the winit window to the HTML `<canvas id="groot-canvas">` element.
///
/// On non-WASM targets this is a no-op so the call site stays uniform.
pub fn mount_canvas(window: &Window) {
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;

        let canvas = window.canvas().expect("Failed to get winit canvas");
        let document = web_sys::window()
            .and_then(|win| win.document())
            .expect("Failed to get HTML document");
        let body = document.body().expect("Failed to get HTML body");

        canvas.set_id("groot-canvas");
        let _ = body.append_child(&canvas);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = window;
    }
}