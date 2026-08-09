mod groot_module;
mod groot_plugin;

use bevy::prelude::*;
use groot_plugin::{GrootConfig, GrootPlugin};

fn main() {
    // Everything about the game — window, prefabs (visuals), and the initial
    // scene — is declared as *data* in groot.toml. No Rust is needed to define
    // what anything looks like; scripts only provide behavior.
    let config = GrootConfig::load("groot.toml");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: config.window.title.clone(),
                resolution: (config.window.width, config.window.height).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GrootPlugin)
        .insert_resource(config)
        .add_systems(Startup, setup_camera)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
