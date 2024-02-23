use bevy::prelude::*;

use crate::{asset_paths, GameState};

pub struct HowToPlayPlugin;

impl Plugin for HowToPlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::HowToPlay), setup);
        app.add_systems(OnExit(GameState::HowToPlay), teardown);

        app.add_systems(
            Update,
            process_input_system.run_if(in_state(GameState::HowToPlay)),
        );
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::MULTIPLAYER_MENU),
            settings: PlaybackSettings::LOOP,
            ..default()
        },
        HowToPlayMusic,
    ));
}

fn teardown(mut commands: Commands, texts: Query<Entity, With<HowToPlayMusic>>) {
    for text in texts.iter() {
        commands.entity(text).despawn_recursive();
    }
}

fn process_input_system(
    mut game_state: ResMut<NextState<GameState>>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
    }
}

#[derive(Debug, Default, Component)]
struct HowToPlayMusic;
