pub mod host;
pub mod input;

pub use host::{sync_camera_commands, update_scripts, GrootScriptHost};
#[allow(unused_imports)]
pub use input::InputState;