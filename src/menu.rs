use bevy::prelude::*;
use strum::{EnumIter, IntoEnumIterator};

use crate::{asset_paths, sounds::SoundEvent, GameState};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Menu), setup_system);
        app.add_systems(OnExit(GameState::Menu), tear_down_system);

        app.add_systems(
            Update,
            process_input_system.run_if(in_state(GameState::Menu)),
        );
    }
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash, EnumIter)]
pub enum MenuOptions {
    #[default]
    LocalPlay,
    ComputerPlay,
    Multiplayer,
    HowToPlay,
    Credits,
}

pub trait Changeable: Copy + Eq + IntoEnumIterator {
    fn change(self, delta: i32) -> Self {
        let options = Self::iter().collect::<Vec<_>>();
        let index = options
            .iter()
            .position(|&option| option == self)
            .expect("Current enum variant not found in iterator");

        let options_len = options.len() as i32;
        let new_index = ((index as i32 + delta) % options_len + options_len) % options_len;

        options[new_index as usize]
    }
}

impl Changeable for MenuOptions {}

impl ToString for MenuOptions {
    fn to_string(&self) -> String {
        match self {
            MenuOptions::LocalPlay => "Fast Play",
            MenuOptions::Credits => "Credits",
            MenuOptions::Multiplayer => "Net Play",
            MenuOptions::ComputerPlay => "Com Play",
            MenuOptions::HowToPlay => "How Play?",
        }
        .to_string()
    }
}

#[derive(Debug, Default, Component)]
pub struct MenuInput {
    pub selected: MenuOptions,
}

#[derive(Debug, Default, Component)]
struct MenuMusic;

fn setup_system(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(MenuInput::default());

    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::MAIN_MENU),
            settings: PlaybackSettings::LOOP,
            ..default()
        },
        MenuMusic,
    ));
}

fn tear_down_system(
    mut commands: Commands,
    menu_input: Query<Entity, With<MenuInput>>,
    menu_music: Query<Entity, With<MenuMusic>>,
) {
    for entity in menu_input.iter() {
        commands.entity(entity).despawn_recursive();
    }

    for entity in menu_music.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

fn process_input_system(
    mut menu_input: Query<&mut MenuInput>,
    mut game_state: ResMut<NextState<GameState>>,
    mut sound_events: EventWriter<SoundEvent>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    for mut input in menu_input.iter_mut() {
        if keyboard_input.just_pressed(KeyCode::Up) {
            input.selected = input.selected.change(-1);
            sound_events.send(SoundEvent::MoveMenu);
        }

        if keyboard_input.just_pressed(KeyCode::Down) {
            input.selected = input.selected.change(1);
            sound_events.send(SoundEvent::MoveMenu);
        }

        if keyboard_input.just_pressed(KeyCode::Return) {
            match input.selected {
                MenuOptions::LocalPlay => game_state.set(GameState::PlayLocal),
                MenuOptions::Credits => game_state.set(GameState::Credits),
                MenuOptions::Multiplayer => game_state.set(GameState::Multiplayer),
                MenuOptions::ComputerPlay => game_state.set(GameState::ComputerPlay),
                MenuOptions::HowToPlay => game_state.set(GameState::HowToPlay),
            }

            sound_events.send(SoundEvent::Select);
        }
    }
}
