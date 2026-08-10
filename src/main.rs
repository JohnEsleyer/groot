mod groot_module;
mod groot_plugin;

use bevy::prelude::*;
use groot_plugin::{GrootConfig, GrootPlugin};

fn main() {
    let config = GrootConfig::load("assets/config.ron");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: config.window.title.clone(),
                resolution: (config.window.width, config.window.height).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(config.render.clear_color.to_color()))
        .add_plugins(GrootPlugin)
        .insert_resource(config)
        .run();
}
