use crate::{
    sounds::SoundEvent,
    uchess::{AlgebraicMoves, MoveEvent, PlayerActive, PlayerBundle, PlayerTeam},
    GameState,
};
use bevy::prelude::*;
use std::collections::HashMap;
use ternary_tree::Tst;

pub struct LocalInputPlugin;

impl Plugin for LocalInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<AlgebraicNotationInputEvent>();

        app.add_systems(OnEnter(GameState::PlayLocal), setup_play_local);

        app.add_systems(OnEnter(GameState::Playing), setup_playing);

        app.add_systems(
            Update,
            (
                process_algebraic_notation_system,
                key_press_algebraic_input,
                key_press_options,
            )
                .run_if(in_state(GameState::Playing)),
        );
    }
}

fn setup_play_local(mut commands: Commands, mut game_state: ResMut<NextState<GameState>>) {
    commands
        .spawn(PlayerBundle {
            team: PlayerTeam::White,
            ..default()
        })
        .insert(LocalPlayerInput);

    commands
        .spawn(PlayerBundle {
            team: PlayerTeam::Black,
            ..default()
        })
        .insert(LocalPlayerInput);

    game_state.set(GameState::Playing);
}

fn setup_playing(mut commands: Commands, existing: Query<Entity, With<AlgebraicNotationInput>>) {
    for entity in existing.iter() {
        commands.entity(entity).despawn_recursive();
    }

    commands.spawn((AlgebraicNotationInput {
        current_input: String::new(),
        auto_complete: Vec::new(),
    },));

    commands.remove_resource::<AlgebraicMoveHistory>();
    commands.insert_resource::<AlgebraicMoveHistory>(AlgebraicMoveHistory::default());
}

#[derive(Debug, Clone, Component)]
pub struct LocalPlayerInput;

#[derive(Debug, Clone, Resource, Default)]
pub struct AlgebraicMoveHistory {
    pub moves: Vec<String>,
}

#[derive(Debug, Clone, Event)]
pub struct AlgebraicNotationInputEvent {
    pub algebraic_notation: String,
    pub team: PlayerTeam,
}

impl AlgebraicNotationInputEvent {
    pub fn new(algebraic_notation: String, team: PlayerTeam) -> Self {
        Self {
            algebraic_notation,
            team,
        }
    }
}

fn process_algebraic_notation_system(
    mut an_reader: EventReader<AlgebraicNotationInputEvent>,
    mut pm_writer: EventWriter<MoveEvent>,
    mut algebraic_move_history: ResMut<AlgebraicMoveHistory>,
    algebraic_moves: Res<AlgebraicMoves>,
) {
    let possible_moves = &algebraic_moves.moves;

    for an in an_reader.read() {
        debug!("Algebraic Notation {:?}", an);

        if !possible_moves.contains_key(&an.team) {
            continue;
        }

        if let Some(mov) = possible_moves[&an.team].get(&an.algebraic_notation) {
            pm_writer.send(MoveEvent::new(*mov));

            algebraic_move_history
                .moves
                .push(an.algebraic_notation.clone());
        }
    }

    an_reader.clear();
}

#[derive(Debug, Component, Clone)]
pub struct AlgebraicNotationInput {
    pub current_input: String,
    pub auto_complete: Vec<String>,
}

pub fn key_code_to_string(key: KeyCode) -> &'static str {
    const NUMBERS: [&'static str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
    if key as u8 >= KeyCode::Key1 as u8 && key as u8 <= KeyCode::Key0 as u8 {
        return NUMBERS[(key as u8 - KeyCode::Key1 as u8) as usize];
    }

    const ALPHABET: [&'static str; 26] = [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t", "u", "v", "w", "x", "y", "z",
    ];

    if key as u8 >= KeyCode::A as u8 && key as u8 <= KeyCode::Z as u8 {
        let offset = (key as u8 - KeyCode::A as u8) as usize;
        return ALPHABET[offset];
    }

    if key == KeyCode::Minus {
        return "-";
    }

    if key == KeyCode::Equals {
        return "=";
    }

    ""
}

fn make_move_tree<I, T>(possible_moves: I) -> Tst<String>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str> + ToString,
{
    let mut move_tree = Tst::new();

    for mov in possible_moves {
        let value = mov.to_string();
        let key = mov.to_string();
        move_tree.insert(&key, value);
    }

    move_tree
}

fn get_possible_moves<I, T>(input: &str, possible_moves: I) -> Vec<String>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str> + ToString,
{
    let move_tree = make_move_tree(possible_moves);

    let mut result = Vec::new();
    move_tree.visit_complete_values(input, |value| {
        result.push(value.to_string());
    });

    result
}

fn key_press_options(
    keyboard_input: Res<Input<KeyCode>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
    }
}

fn key_press_algebraic_input(
    keyboard_input: Res<Input<KeyCode>>,
    mut input: Query<&mut AlgebraicNotationInput>,
    mut an_input_writer: EventWriter<AlgebraicNotationInputEvent>,
    algebraic_moves: Res<AlgebraicMoves>,
    mut caps: Local<bool>,
    mut sound_events: EventWriter<SoundEvent>,
    player_inputs: Query<&PlayerTeam, (With<PlayerActive>, With<LocalPlayerInput>)>,
) {
    let team = match player_inputs.get_single() {
        Ok(value) => value,
        Err(_) => return,
    };

    let mut input = input.single_mut();

    if keyboard_input.just_released(KeyCode::ShiftLeft)
        || keyboard_input.just_released(KeyCode::ShiftRight)
    {
        *caps = false;
    }
    if keyboard_input.just_pressed(KeyCode::ShiftLeft)
        || keyboard_input.just_pressed(KeyCode::ShiftRight)
    {
        *caps = true;
    }

    let empty = HashMap::new();

    let possible_algebraic_moves = &algebraic_moves.moves;
    let possible_algebraic_moves = match possible_algebraic_moves.get(team) {
        Some(value) => value,
        None => &empty,
    };

    let mut auto_complete_dirty = false;

    let old_input = input.current_input.clone();

    if keyboard_input.just_pressed(KeyCode::Return) {
        if !possible_algebraic_moves.contains_key(&input.current_input) {
            sound_events.send(SoundEvent::Error);
        }

        an_input_writer.send(AlgebraicNotationInputEvent {
            algebraic_notation: input.current_input.clone(),
            team: *team,
        });
        input.current_input = String::new();
        input.auto_complete = Vec::new();
    }

    if keyboard_input.just_pressed(KeyCode::Back) {
        let mut chars = input.current_input.chars();
        chars.next_back();
        input.current_input = chars.collect::<String>();
        auto_complete_dirty = true;
        sound_events.send(SoundEvent::Backspace);
    }

    if keyboard_input.just_pressed(KeyCode::Tab) {
        if input.auto_complete.len() <= 0 {
            sound_events.send(SoundEvent::Error);
        } else {
            input.current_input = input.auto_complete[0].clone();
            auto_complete_dirty = true;
            sound_events.send(SoundEvent::KeyInput);
        }
    }

    let mut input_dirty = false;

    keyboard_input.get_just_pressed().for_each(|key| {
        let next_key = if *caps {
            key_code_to_string(*key).to_uppercase()
        } else {
            key_code_to_string(*key).to_lowercase()
        };

        if next_key != "" {
            auto_complete_dirty = true;
            input_dirty = true;
            input.current_input = format!("{}{}", input.current_input, next_key);
        }
    });

    let old_possibles = input.auto_complete.clone();
    if auto_complete_dirty {
        if input.current_input.len() > 0 {
            let possibilities =
                get_possible_moves(&input.current_input, possible_algebraic_moves.keys());
            input.auto_complete = possibilities;
        } else {
            input.auto_complete = Vec::new();
        }
    }

    if input_dirty {
        if input.auto_complete.len() == 0 {
            if possible_algebraic_moves.contains_key(&input.current_input) {
                sound_events.send(SoundEvent::KeyInput);
            } else {
                input.current_input = old_input;
                sound_events.send(SoundEvent::Error);
                input.auto_complete = old_possibles;
            }
        } else {
            sound_events.send(SoundEvent::KeyInput);
        }
    }
}
