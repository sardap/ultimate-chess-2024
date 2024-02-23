use std::time::Duration;

use bevy::prelude::*;

use crate::{
    asset_paths,
    render::{LongTextScroller, STAGE_SIZE},
    uchess::Position,
    GameState,
};

pub struct CreditsPlugin;

impl Plugin for CreditsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Credits), setup);
        app.add_systems(OnExit(GameState::Credits), teardown);

        app.add_systems(
            Update,
            (process_input_system, tick_credit_text).run_if(in_state(GameState::Credits)),
        );
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let texts: Vec<(&'static str, Duration)> = vec![
        ("Ultimate Chess 2024", Duration::from_secs(0)),
        (
            "Chess: Invented somewhere in India during the 6th century",
            Duration::from_secs(4),
        ),
        ("Programming: Paul Sarda", Duration::from_secs(8)),
        (
            "Music: From 8bit Music Pack by Marcelo Fernandez",
            Duration::from_secs(12),
        ),
    ];

    let white_space_prefix = " ".repeat((STAGE_SIZE.x / 2) as usize);

    for (i, (text, spawn_time)) in texts.iter().enumerate() {
        commands.spawn(CreditTextBundle {
            text: CreditText {
                text: format!("{}{}", white_space_prefix, text.to_string()),
            },
            position: Position::new(0, i as i32),
            scroller: LongTextScroller::new(text),
            spawn_timer: SpawnTimer(Timer::from_seconds(
                spawn_time.as_secs_f32(),
                TimerMode::Once,
            )),
            ..default()
        });
    }

    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::CREDITS),
            settings: PlaybackSettings::LOOP,
            ..default()
        },
        CreditMusic,
    ));
}

fn teardown(
    mut commands: Commands,
    texts: Query<Entity, Or<(With<CreditText>, With<CreditMusic>)>>,
) {
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
struct CreditMusic;

#[derive(Debug, Component)]
pub struct CreditText {
    pub text: String,
}

impl Default for CreditText {
    fn default() -> Self {
        Self {
            text: "".to_string(),
        }
    }
}

#[derive(Debug, Default, Component)]
pub struct SpawnTimer(Timer);

#[derive(Debug, Default, Component)]
pub struct Invisible;

#[derive(Debug, Default, Bundle)]
pub struct CreditTextBundle {
    pub text: CreditText,
    pub position: Position,
    pub scroller: LongTextScroller,
    pub spawn_timer: SpawnTimer,
    pub invisible: Invisible,
}

fn tick_credit_text(
    mut commands: Commands,
    time: Res<Time>,
    mut text: Query<
        (Entity, &mut SpawnTimer, &mut LongTextScroller),
        (With<Invisible>, With<CreditText>),
    >,
) {
    for (entity, mut spawn_timer, mut text_scroller) in text.iter_mut() {
        text_scroller.reset();

        spawn_timer.0.tick(time.delta());

        if spawn_timer.0.finished() {
            commands.entity(entity).remove::<Invisible>();
        }
    }
}
