use winit::event_loop::EventLoop;

/// Create a native (X11/Wayland on Linux, Win32 on Windows, AppKit on macOS)
/// event loop for the current desktop host.
pub fn create_event_loop() -> EventLoop<()> {
    EventLoop::new().expect("Failed to create desktop EventLoop")
}