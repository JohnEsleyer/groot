use std::sync::Arc;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

pub struct WindowManager {
    window: Option<Arc<Window>>,
}

impl WindowManager {
    pub fn new() -> Self {
        Self { window: None }
    }

    pub fn create_window(
        &mut self,
        el: &EventLoop<()>,
        width: u32,
        height: u32,
        title: &str,
        resizable: bool,
    ) {
        let win = WindowBuilder::new()
            .with_title(title)
            .with_inner_size(LogicalSize::new(width as f64, height as f64))
            .with_min_inner_size(LogicalSize::new(160.0, 120.0))
            .with_resizable(resizable)
            .build(el)
            .expect("Failed to create winit window");
        self.window = Some(Arc::new(win));
    }

    pub fn window(&self) -> Option<Arc<Window>> {
        self.window.clone()
    }

    pub fn is_open(&self) -> bool {
        self.window.is_some()
    }
}
