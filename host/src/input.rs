//! Host-side input capture.
//!
//! Groot's Ube input package consumes a typed snapshot of these events. The
//! queue is filled by the winit event loop and is kept in host state so a
//! future WIT `input` interface (or an exported guest tick) can drain it
//! without changing the engine-side `InputManager` API.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub struct RawInputEvent {
    /// 0: KeyDown, 1: KeyUp, 2: MouseMove, 3: MouseDown, 4: MouseUp
    pub event_type: u32,
    pub key_code: u32,
    pub mouse_x: f32,
    pub mouse_y: f32,
}

#[derive(Default)]
pub struct InputQueue {
    events: Vec<RawInputEvent>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub keys_down: HashSet<u32>,
    pub mouse_down: [bool; 4],
}

impl InputQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_key(&mut self, code: u32, pressed: bool) {
        if pressed {
            self.keys_down.insert(code);
        } else {
            self.keys_down.remove(&code);
        }
    }

    pub fn on_mouse_button(&mut self, btn: u32, pressed: bool) {
        if (btn as usize) < self.mouse_down.len() {
            self.mouse_down[btn as usize] = pressed;
        }
    }

    pub fn push(&mut self, ev: RawInputEvent) {
        // Keep the queue bounded so a headless guest cannot grow host memory
        // without bound.
        const MAX_QUEUED_EVENTS: usize = 4096;
        if self.events.len() >= MAX_QUEUED_EVENTS {
            self.events.remove(0);
        }
        self.events.push(ev);
    }

    pub fn set_mouse(&mut self, x: f32, y: f32) {
        self.mouse_x = x;
        self.mouse_y = y;
    }

    pub fn drain(&mut self) -> Vec<RawInputEvent> {
        std::mem::take(&mut self.events)
    }
}
