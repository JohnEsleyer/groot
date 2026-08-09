mod groot_module;
mod groot_plugin;

use bevy::prelude::*;
use groot_plugin::{GoScriptComponent, GrootPlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Groot Engine — Hybrid Component-Behavior Architecture".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GrootPlugin)
        .add_systems(Startup, setup_game)
        .run();
}

fn setup_game(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

    // Player Entity (ID #1) — Groot Green
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::rgb(0.1, 0.8, 0.3),
                custom_size: Some(Vec2::new(60.0, 60.0)),
                ..default()
            },
            transform: Transform::from_xyz(-100.0, 0.0, 0.0),
            ..default()
        },
        GoScriptComponent {
            script_path: "assets/scripts/player.gs".into(),
            entity_id: 1,
            tag: "Player".into(),
        },
    ));

    // Enemy Entity (ID #2) — Purple
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::rgb(0.8, 0.2, 0.8),
                custom_size: Some(Vec2::new(60.0, 60.0)),
                ..default()
            },
            transform: Transform::from_xyz(100.0, 0.0, 0.0),
            ..default()
        },
        GoScriptComponent {
            script_path: "assets/scripts/enemy.gs".into(),
            entity_id: 2,
            tag: "Enemy".into(),
        },
    ));
}
