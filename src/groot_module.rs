use bevy::log::{info, warn};
use goscript::{VirtualMachine, Value};

/// Extends a `goscript::VirtualMachine` with the Groot engine API surface.
///
/// This keeps the `goscript` crate 100% engine-agnostic (pure Go language
/// plus its `math`/`fmt`/`rand`/`time` stdlib) while Groot injects its own
/// `groot.*` bindings into the VM instance.
pub trait GrootModuleExt {
    fn register_groot_module(&mut self);
}

impl GrootModuleExt for VirtualMachine {
    fn register_groot_module(&mut self) {
        // --- Groot logging API -------------------------------------------------
        self.register_fn("groot.Log", |args| {
            let msg: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            info!("[GROOT LOG]: {}", msg.join(" "));
            Value::Nil
        });

        self.register_fn("groot.Warn", |args| {
            let msg: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            warn!("[GROOT WARN]: {}", msg.join(" "));
            Value::Nil
        });

        // --- Groot input API ---------------------------------------------------
        // Registered once as no-ops; the frame system re-registers these with
        // the live keyboard state captured in closures (see groot_plugin).
        self.register_fn("groot.GetAxis", |args| match args.first() {
            Some(Value::String(axis)) if axis == "Horizontal" => Value::Float(0.0),
            Some(Value::String(axis)) if axis == "Vertical" => Value::Float(0.0),
            _ => Value::Float(0.0),
        });

        self.register_fn("groot.IsKeyDown", |_| Value::Bool(false));

        // --- Groot ECS / entity API --------------------------------------------
        self.register_fn("groot.SpawnEntity", |args| {
            if let Some(Value::String(prefab_name)) = args.first() {
                info!("[GROOT ECS]: Spawning prefab '{}'", prefab_name);
            }
            Value::Nil
        });

        self.register_fn("groot.DestroyEntity", |args| {
            if let Some(Value::Int(id)) = args.first() {
                info!("[GROOT ECS]: Destroying entity #{id}");
            }
            Value::Nil
        });

        // --- Groot audio API ---------------------------------------------------
        self.register_fn("groot.PlaySound", |args| {
            if let Some(Value::String(sound_path)) = args.first() {
                info!("[GROOT AUDIO]: Playing sound file {sound_path}");
            }
            Value::Nil
        });
    }
}