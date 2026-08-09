mod groot_module;
mod groot_plugin;

use bevy::prelude::*;
use groot_plugin::{
    Bird, GoScriptComponent, GrootPlugin, PipeIndex, ScoreText, ScriptColor, ScriptTransform,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Groot Flappy Bird (GoScript + Bevy ECS)".into(),
                resolution: (800.0, 600.0).into(),
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

    // Bird Entity — driven by flappy.gs via `groot.SetPosition`
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::rgb(0.95, 0.8, 0.2),
                custom_size: Some(Vec2::new(32.0, 32.0)),
                ..default()
            },
            transform: Transform::from_xyz(-50.0, 0.0, 10.0),
            ..default()
        },
        Bird,
        GoScriptComponent {
            script_path: "assets/scripts/flappy.gs".into(),
            entity_id: 1,
            tag: "Bird".into(),
        },
        ScriptTransform {
            x: -50.0,
            y: 0.0,
            ..default()
        },
        ScriptColor(Color::rgb(0.95, 0.8, 0.2)),
    ));

    // Score HUD — top-center text, updated by flappy.gs via SetScoreDisplay
    commands.spawn((
        Text2dBundle {
            text: Text::from_section(
                "Score: 0  Best: 0",
                TextStyle {
                    font_size: 28.0,
                    color: Color::WHITE,
                    ..default()
                },
            )
            .with_justify(JustifyText::Center),
            transform: Transform::from_xyz(0.0, 260.0, 20.0),
            ..default()
        },
        ScoreText,
    ));

    // Ground — visual green strip at bottom
    commands.spawn(SpriteBundle {
        sprite: Sprite {
            color: Color::rgb(0.2, 0.7, 0.3),
            custom_size: Some(Vec2::new(800.0, 100.0)),
            ..default()
        },
        transform: Transform::from_xyz(0.0, -260.0, 5.0),
        ..default()
    });

    // Pipe pairs — 3 pairs (index 0..5), even=top, odd=bottom
    // Positioned off-screen initially; GoScript will move them via SetPipePosition
    for i in 0..6 {
        commands.spawn((
            SpriteBundle {
                sprite: Sprite {
                    color: Color::rgb(0.1, 0.8, 0.3),
                    custom_size: Some(Vec2::new(52.0, 400.0)),
                    ..default()
                },
                transform: Transform::from_xyz(1000.0, 0.0, 2.0),
                ..default()
            },
            PipeIndex(i),
        ));
    }
}
