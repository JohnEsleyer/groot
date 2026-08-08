use std::sync::Mutex;

use bevy::prelude::*;
use goscript::{HotReloadEngine, Value};

use crate::groot_module::GrootModuleExt;

/// Component attached to any Bevy entity driven by a GoScript file.
#[derive(Component)]
pub struct GoScriptComponent {
    pub script_path: String,
}

/// Resource holding the GoScript hot-reload VM and script bookkeeping.
pub struct GrootScriptHost {
    engine: HotReloadEngine,
    script_path: String,
}

/// Shared player translation written by the `groot.SetPosition` binding and
/// read by the transform-sync system. Native bindings registered into the VM
/// cannot borrow the Bevy world; the position crosses the boundary through
/// this process-wide slot instead.
static PLAYER_POSITION: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));

impl GrootScriptHost {
    pub fn new(script_path: &str) -> Self {
        let mut engine = HotReloadEngine::new(script_path);
        let vm = &mut engine.vm;

        // Inject the whole `groot.*` API surface (Log, Warn, GetAxis,
        // SpawnEntity, DestroyEntity, PlaySound...) from the engine module.
        vm.register_groot_module();

        // Engine-frame state that the groot module deliberately does not own:
        // the mutable player position written by scripts via `groot.SetPosition`.
        vm.register_fn("groot.SetPosition", |args| {
            let x = args
                .first()
                .and_then(|v| v.as_number())
                .unwrap_or(0.0) as f32;
            let y = args
                .get(1)
                .and_then(|v| v.as_number())
                .unwrap_or(0.0) as f32;
            if let Ok(mut pos) = PLAYER_POSITION.lock() {
                *pos = (x, y);
            }
            Value::Nil
        });

        Self {
            engine,
            script_path: script_path.to_string(),
        }
    }

    /// Recompile the script if the `.go` file changed on disk. Globals already
    /// in the VM survive the swap (work-in-progress game state is preserved).
    fn reload_if_changed(&mut self) {
        if let Err(e) = self.engine.reload_if_changed() {
            error!("[GROOT HOT-RELOAD]: {e}");
        }
    }

    /// Push the current frame's input state into the VM, then run the script's
    /// `OnUpdate(dt)` function through the live VM.
    fn push_input_and_tick(&mut self, move_x: f64, move_y: f64, space: bool, dt: f64) {
        let vm = &mut self.engine.vm;

        vm.register_fn("groot.GetAxis", move |args| {
            if let Some(Value::String(axis)) = args.first() {
                if axis == "Horizontal" {
                    return Value::Float(move_x);
                }
                if axis == "Vertical" {
                    return Value::Float(move_y);
                }
            }
            Value::Float(0.0)
        });

        vm.register_fn("groot.IsKeyDown", move |args| {
            if let Some(Value::String(key)) = args.first() {
                if key == "Space" {
                    return Value::Bool(space);
                }
            }
            Value::Bool(false)
        });

        vm.set_delta_time(dt);
        if let Err(e) = vm.call("OnUpdate", vec![Value::Float(dt)]) {
            error!("[GROOT SCRIPT]: OnUpdate failed: {e}");
        }
    }
}

pub struct GrootPlugin;

impl Plugin for GrootPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GrootScriptHost::new("assets/scripts/player.go"))
            .add_systems(
                Update,
                (
                    script_hot_reload_system,
                    script_update_system,
                    sync_transforms_system,
                ),
            );
    }
}

fn script_hot_reload_system(mut host: ResMut<GrootScriptHost>) {
    host.reload_if_changed();
}

fn script_update_system(
    mut host: ResMut<GrootScriptHost>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let mut move_x = 0.0;
    let mut move_y = 0.0;

    if keyboard_input.pressed(KeyCode::KeyD) || keyboard_input.pressed(KeyCode::ArrowRight) {
        move_x += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) || keyboard_input.pressed(KeyCode::ArrowLeft) {
        move_x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyW) || keyboard_input.pressed(KeyCode::ArrowUp) {
        move_y += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) || keyboard_input.pressed(KeyCode::ArrowDown) {
        move_y -= 1.0;
    }
    let space = keyboard_input.pressed(KeyCode::Space);

    host.push_input_and_tick(move_x, move_y, space, time.delta_seconds() as f64);
}

fn sync_transforms_system(mut query: Query<&mut Transform, With<GoScriptComponent>>) {
    let pos = match PLAYER_POSITION.lock() {
        Ok(guard) => *guard,
        Err(_) => (0.0, 0.0),
    };
    for mut transform in &mut query {
        transform.translation.x = pos.0;
        transform.translation.y = pos.1;
    }
}