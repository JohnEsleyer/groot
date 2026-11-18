# Case Study 001: Android Splash-Screen Hang — Deferred GPU Surface Creation

## TL;DR

The Groot engine's APK launched on a physical Android device but froze on the
native splash screen. The `RenderContext` (wgpu surface + pipeline) was being
created *before* the winit event loop started running. On Android the
`ANativeWindow` does not exist at that point, so `create_surface_unsafe`
failed with `Err(Unavailable)`, which the engine unwrapped into a panic on a
background thread. The fix moves all GPU/graphics initialization into the
`Event::Resumed` handler, which only fires once the native window surface is
live.

## Symptoms

- APK installs, launches, and shows the default splash screen — and stays there.
- `adb logcat` shows a panic on thread `<unnamed>` (the app glue thread):

```
E winit::platform_impl: Cannot get the native window, it's null and will
  always be null before Event::Resumed and after Event::Suspended. Make sure
  you only call this function between those events.
E RustPanic: called `Result::unwrap()` on an `Err` value: Unavailable
```

- The very next log line (`Starting Groot Engine on Android`) shows the engine
  did boot; it died moments later at `src/render/context.rs:27:69`.

## Root Cause

On Android, the application window surface is *not* available synchronously.
The lifecycle is:

1. `android_main` is invoked with an `AndroidApp` handle.
2. A winit `EventLoop` is built and handed to `run_game_with_event_loop`.
3. The native window only materializes when the OS delivers
   `Event::Resumed` to the loop.

The old event-loop runner created the `RenderContext` (which calls
`instance.create_surface_unsafe(SurfaceTargetUnsafe::from_window(...))`)
immediately, before `event_loop.run(...)`. On desktop this happens to work
because the window is created eagerly; on Android the surface is not there
yet, wgpu returns `Unavailable`, and `.expect(...)` panicked on the native
glue thread. The main (Java/UI) thread kept showing the splash screen because
the render thread never got a chance to present a frame.

```
before:  event_loop.run(|...| {
             RenderContext::new()   // <-- too early on Android
             ...
         })
```

## Fix: Lazy GPU Initialization Inside `Event::Resumed`

Refactor `run_game_with_event_loop` so all GPU resources live in an optional
`RenderState`, built on the first `Event::Resumed`:

```rust
struct RenderState<'a> {
    render_ctx:      RenderContext<'a>,
    pipeline_3d:     Pipeline3D,
    camera:          Camera3D,
    camera_buffer:   wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    cube_mesh:       Mesh,
    world:           World,
    script_host:     GrootScriptHost,
}
```

The loop becomes:

```rust
let mut state: Option<RenderState> = None;

event_loop.run(move |event, target| match event {
    Event::Resumed => {
        if state.is_none() {
            let render_ctx = pollster::block_on(RenderContext::new(
                Arc::clone(&window), config.render.clear_color));
            // ... build pipeline, camera, scene, script host ...
            state = Some(RenderState { /* ... */ });
        }
    }
    Event::Suspended => {
        // surface is gone; next Resumed re-creates it
    }
    Event::WindowEvent { event, .. } => { /* handle against state */ }
    _ => {}
});
```

Key points:

- `RenderState` is `Option` so it can be created on resume and (if needed)
  dropped on suspend.
- `Event::Resumed` is the *only* place the window surface is guaranteed to
  exist — on both Android and desktop.
- `RedrawRequested` guards on `if let Some(ref mut st) = state`, so the first
  redraw request before a resume is a no-op rather than a crash.

## Why Desktop Didn't Catch This

`run_game` for non-Android targets builds the window and event loop eagerly,
and the window is visible before any redraw happens. The lazy `Resumed` path
is a superset: it works on desktop too (Resumed always fires once the window
is ready), so a single code path now serves both platforms.

## Result

- The engine boots, waits for the surface, initializes the wgpu pipeline on
  the first `Resumed`, and starts rendering immediately afterward.
- No more `Err: Unavailable` panic.
- The same pattern also handles suspend/resume correctly (e.g. when the app is
  backgrounded and restored on Android).

## Related Fixes in the Android Bring-Up

Getting to a runnable APK required several prior fixes on the toolchain side:

1. `cargo-apk` requires a `cdylib` entry point, not a bin target — the CLI
   passes `--lib` for Android (`src/lib.rs` exports `android_main`).
2. `cargo apk run` was pointed at the selected device via `--device <serial>`
   to avoid `adb: more than one device/emulator`.
3. `android-activity` 0.5 panics on a NULL `savedState` pointer under Rust
   1.78+ strict non-null checks; fixed by disabling debug assertions for that
   crate via `[profile.dev.package.android-activity] debug-assertions = false`
   (bumping to 0.6 is not possible while winit 0.29 pins 0.5).

## Files Touched

- `src/lib.rs` — moved GPU/scene/script init into `Event::Resumed` via
  `Option<RenderState>`.
- `src/render/context.rs` — unchanged; the panic site that surfaced the bug.
- `src/platform/android.rs` — unchanged; the `android_main` entry point.
- `src/bin/cli.rs` — `--lib` for Android, `--device` pass-through.
- `Cargo.toml` — debug-assertions override for `android-activity`.
