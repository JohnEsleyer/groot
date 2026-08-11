use std::cell::{Cell, RefCell};
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub struct InputState {
    pub move_x: Cell<f64>,
    pub move_y: Cell<f64>,
    pub keys_down: RefCell<Vec<String>>,
    pub keys_just_pressed: RefCell<Vec<String>>,
    pub mouse_pos: Cell<(f64, f64)>,
    pub mouse_button_down: Cell<[bool; 3]>,
    pub mouse_button_pressed: Cell<[bool; 3]>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            move_x: Cell::new(0.0),
            move_y: Cell::new(0.0),
            keys_down: RefCell::new(Vec::new()),
            keys_just_pressed: RefCell::new(Vec::new()),
            mouse_pos: Cell::new((0.0, 0.0)),
            mouse_button_down: Cell::new([false; 3]),
            mouse_button_pressed: Cell::new([false; 3]),
        }
    }

    pub fn reset_frame_input(&self) {
        self.keys_just_pressed.borrow_mut().clear();
        self.mouse_button_pressed.set([false; 3]);
    }

    pub fn process_event(&self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let key_str = match_key_code(code);
                    if let Some(key_name) = key_str {
                        let mut down = self.keys_down.borrow_mut();
                        let mut pressed = self.keys_just_pressed.borrow_mut();

                        if event.state == ElementState::Pressed {
                            if !down.contains(&key_name) {
                                down.push(key_name.clone());
                                pressed.push(key_name);
                            }
                        } else {
                            down.retain(|k| k != &key_name);
                        }
                    }
                }
                self.recalculate_axes();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos.set((position.x, position.y));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let idx = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => 0,
                };
                let is_pressed = *state == ElementState::Pressed;
                let mut down = self.mouse_button_down.get();
                let mut pressed = self.mouse_button_pressed.get();

                if is_pressed && !down[idx] {
                    pressed[idx] = true;
                }
                down[idx] = is_pressed;

                self.mouse_button_down.set(down);
                self.mouse_button_pressed.set(pressed);
            }
            // Mobile touchscreen support: map touch events to mouse button 0 so
            // GoScript `groot.IsMouseDown(0)` and cursor queries keep working.
            WindowEvent::Touch(touch) => {
                self.mouse_pos.set((touch.location.x, touch.location.y));
                let idx = 0;
                let mut down = self.mouse_button_down.get();
                let mut pressed = self.mouse_button_pressed.get();

                match touch.phase {
                    TouchPhase::Started => {
                        if !down[idx] {
                            pressed[idx] = true;
                        }
                        down[idx] = true;
                    }
                    TouchPhase::Moved => {
                        down[idx] = true;
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        down[idx] = false;
                    }
                }

                self.mouse_button_down.set(down);
                self.mouse_button_pressed.set(pressed);
            }
            _ => {}
        }
    }

    fn recalculate_axes(&self) {
        let down = self.keys_down.borrow();
        let mut mx = 0.0f64;
        let mut my = 0.0f64;

        if down.contains(&"KeyD".to_string()) || down.contains(&"ArrowRight".to_string()) {
            mx += 1.0;
        }
        if down.contains(&"KeyA".to_string()) || down.contains(&"ArrowLeft".to_string()) {
            mx -= 1.0;
        }
        if down.contains(&"KeyW".to_string()) || down.contains(&"ArrowUp".to_string()) {
            my += 1.0;
        }
        if down.contains(&"KeyS".to_string()) || down.contains(&"ArrowDown".to_string()) {
            my -= 1.0;
        }

        self.move_x.set(mx);
        self.move_y.set(my);
    }
}

fn match_key_code(code: KeyCode) -> Option<String> {
    match code {
        KeyCode::Space => Some("Space".into()),
        KeyCode::KeyW => Some("KeyW".into()),
        KeyCode::KeyA => Some("KeyA".into()),
        KeyCode::KeyS => Some("KeyS".into()),
        KeyCode::KeyD => Some("KeyD".into()),
        KeyCode::KeyQ => Some("KeyQ".into()),
        KeyCode::KeyE => Some("KeyE".into()),
        KeyCode::KeyR => Some("KeyR".into()),
        KeyCode::ArrowUp => Some("ArrowUp".into()),
        KeyCode::ArrowDown => Some("ArrowDown".into()),
        KeyCode::ArrowLeft => Some("ArrowLeft".into()),
        KeyCode::ArrowRight => Some("ArrowRight".into()),
        _ => None,
    }
}