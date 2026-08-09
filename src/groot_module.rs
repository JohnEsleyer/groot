use goscript::value::Value;
use goscript::vm::VirtualMachine;

/// Stateless Raylib-style utility bindings for the Groot engine.
///
/// These are pure math/collision/logging functions with no entity state.
/// They are registered once on every VM via [`GrootModuleExt::register_groot_module`].
pub trait GrootModuleExt {
    fn register_groot_module(&mut self);
}

impl GrootModuleExt for VirtualMachine {
    fn register_groot_module(&mut self) {
        // --- Logging -----------------------------------------------------------
        self.register_fn("groot.Log", |args| {
            let msg: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            bevy::log::info!("[GROOT LOG]: {}", msg.join(" "));
            Value::Nil
        });

        self.register_fn("groot.Warn", |args| {
            let msg: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            bevy::log::warn!("[GROOT WARN]: {}", msg.join(" "));
            Value::Nil
        });

        self.register_fn("groot.Error", |args| {
            let msg: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            bevy::log::error!("[GROOT ERROR]: {}", msg.join(" "));
            Value::Nil
        });

        // --- Math & Geometry ---------------------------------------------------
        self.register_fn("groot.GetDistance2D", |args| {
            if args.len() >= 4 {
                let x1 = args[0].as_number().unwrap_or(0.0);
                let y1 = args[1].as_number().unwrap_or(0.0);
                let x2 = args[2].as_number().unwrap_or(0.0);
                let y2 = args[3].as_number().unwrap_or(0.0);
                let dx = x2 - x1;
                let dy = y2 - y1;
                return Value::Float((dx * dx + dy * dy).sqrt());
            }
            Value::Float(0.0)
        });

        self.register_fn("groot.Clamp", |args| {
            if args.len() >= 3 {
                let val = args[0].as_number().unwrap_or(0.0);
                let min = args[1].as_number().unwrap_or(0.0);
                let max = args[2].as_number().unwrap_or(0.0);
                return Value::Float(val.clamp(min, max));
            }
            Value::Float(0.0)
        });

        self.register_fn("groot.Lerp", |args| {
            if args.len() >= 3 {
                let a = args[0].as_number().unwrap_or(0.0);
                let b = args[1].as_number().unwrap_or(0.0);
                let t = args[2].as_number().unwrap_or(0.0);
                return Value::Float(a + (b - a) * t);
            }
            Value::Float(0.0)
        });

        // --- Collision ---------------------------------------------------------
        self.register_fn("groot.CheckCollisionRecs", |args| {
            if args.len() >= 8 {
                let x1 = args[0].as_number().unwrap_or(0.0);
                let y1 = args[1].as_number().unwrap_or(0.0);
                let w1 = args[2].as_number().unwrap_or(0.0);
                let h1 = args[3].as_number().unwrap_or(0.0);
                let x2 = args[4].as_number().unwrap_or(0.0);
                let y2 = args[5].as_number().unwrap_or(0.0);
                let w2 = args[6].as_number().unwrap_or(0.0);
                let h2 = args[7].as_number().unwrap_or(0.0);
                return Value::Bool(
                    x1 < x2 + w2 && x1 + w1 > x2 && y1 < y2 + h2 && y1 + h1 > y2,
                );
            }
            Value::Bool(false)
        });

        self.register_fn("groot.CheckCollisionCircles", |args| {
            if args.len() >= 6 {
                let x1 = args[0].as_number().unwrap_or(0.0);
                let y1 = args[1].as_number().unwrap_or(0.0);
                let r1 = args[2].as_number().unwrap_or(0.0);
                let x2 = args[3].as_number().unwrap_or(0.0);
                let y2 = args[4].as_number().unwrap_or(0.0);
                let r2 = args[5].as_number().unwrap_or(0.0);
                let dx = x2 - x1;
                let dy = y2 - y1;
                let dist_sq = dx * dx + dy * dy;
                let radius_sum = r1 + r2;
                return Value::Bool(dist_sq <= radius_sum * radius_sum);
            }
            Value::Bool(false)
        });

        self.register_fn("groot.CheckCollisionCircleRec", |args| {
            if args.len() >= 7 {
                let cx = args[0].as_number().unwrap_or(0.0);
                let cy = args[1].as_number().unwrap_or(0.0);
                let r = args[2].as_number().unwrap_or(0.0);
                let rx = args[3].as_number().unwrap_or(0.0);
                let ry = args[4].as_number().unwrap_or(0.0);
                let rw = args[5].as_number().unwrap_or(0.0);
                let rh = args[6].as_number().unwrap_or(0.0);
                let closest_x = cx.clamp(rx, rx + rw);
                let closest_y = cy.clamp(ry, ry + rh);
                let dx = cx - closest_x;
                let dy = cy - closest_y;
                return Value::Bool((dx * dx + dy * dy) <= (r * r));
            }
            Value::Bool(false)
        });
    }
}
