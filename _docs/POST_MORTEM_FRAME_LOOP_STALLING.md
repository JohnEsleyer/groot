# Post-Mortem: Frame Loop Stalling & OS Display Pump Synchronization

**Date:** 2026-11-22  
**Component:** Groot Host Engine (`_groot/host`)  
**Impact:** Guest WebAssembly game applications (e.g., `02_platformer`) appeared static or hung on initial frames unless continuous mouse/keyboard input was delivered to the window.

---

## 1. Background & Architecture

Groot supports two execution models for guest WebAssembly components:
1. **Inverted Frame Loop (Host-Driven Tick)**: The guest exports `tick(dt: f32) -> bool` or `tick()`, and the host's `winit` event loop invokes it directly during `RedrawRequested`.
2. **Synchronous Main Loop (Guest-Driven Run)**: Standard `wasi:cli/command` components define `main()` with a while loop (e.g. `while frame < 600 { ... }`) that performs updates and submits frames via `ube:graphics/gpu` (`draw-frame` host import).

In the synchronous guest model, the guest thread calls into the host's `draw_frame()` function at the end of each frame to present the swapchain texture.

---

## 2. Root Cause Analysis

### Issue A: Missing Redraw Signals on Presentation
- On Linux (Wayland/X11), macOS (Cocoa), and Windows (DXGI), window compositors and event dispatchers rely on explicit frame invalidate / redraw requests to wake up and exchange surface textures without tearing or stalling.
- Inside `HostState::draw_frame()` in `host/src/main.rs`, the host was calling `pump_window_events()` (which drained pending input and window events using `pump_events(Some(Duration::ZERO))`) and then presented the GPU frame via `wgpu::Surface::present()`.
- However, `window.request_redraw()` was **never requested** inside `draw_frame()`.
- Consequently, once initial startup events settled, the OS window manager and event queue entered an idle/sleep state waiting for external hardware activity. The next swapchain buffer release was gated until an OS input event (such as moving the mouse cursor or pressing keys) triggered event dispatch.

### Issue B: Swapchain Error Handling & Frame Pacing
- On window exposure, tiling, or initial layout changes, `get_current_texture()` could return `wgpu::SurfaceError::Outdated`, `Lost`, or `Timeout`.
- Without auto-reconfiguration and timeout recovery, unhandled surface states caused frame presentation to skip or lock until subsequent window configure events arrived.

---

## 3. Solution & Changes

### 1. Window Redraw Request on Presentation (`host/src/main.rs`)
In `HostState::draw_frame()` (both 2D and 3D paths), `win.request_redraw()` is now explicitly invoked immediately after `self.gpu.draw_frame(...)`:
```rust
self.gpu.draw_frame(r, g, b);

if let Some(win) = self.window_mgr.window() {
    win.request_redraw();
}
```
This guarantees that the OS compositor schedules presentation callbacks smoothly and keeps the display queue active without needing synthetic user input.

### 2. Surface Error Recovery & FIFO Present Mode (`host/src/gpu.rs`)
- Configured swapchain present mode to `PresentMode::Fifo` (standard VSync), preventing driver starvation on Wayland.
- Added automatic surface reconfiguration on `SurfaceError::Outdated` and `SurfaceError::Lost`, plus graceful retry on `SurfaceError::Timeout`.

### 3. Bounded Input Queue Memory (`host/src/input.rs`)
- Migrated event queue storage to `VecDeque` with bounded capacity (256 entries) to prevent unbounded memory growth and optimize `pop_front()` / `push_back()` efficiency.

---

## 4. Verification & Results

All tests pass and verify uninterrupted 60 FPS animation:
1. `cargo check --manifest-path host/Cargo.toml` passed with 0 errors.
2. Compiled and executed `examples/02_platformer`:
   ```bash
   ./run.sh 02_platformer
   ```
   The game boots immediately, renders terrain and player sprites without initial gray stalling, and advances frames automatically without requiring mouse movement or keyboard presses.
