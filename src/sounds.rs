use bevy::{
    audio::{Volume, VolumeLevel},
    prelude::*,
};

use crate::{asset_paths, uchess::PlayerTeam};

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, play_sound);

        app.add_event::<SoundEvent>();
    }
}

#[derive(Debug, Event, PartialEq, Eq, Hash, Clone, Copy)]
pub enum SoundEvent {
    Select,
    MovePiece,
    MoveMenu,
    KeyInput,
    Backspace,
    CapturePiece,
    GameOverWin(PlayerTeam),
    GameOverDraw,
    Error,
    Check,
}

fn play_sound(
    mut commands: Commands,
    mut sound_events: EventReader<SoundEvent>,
    asset_server: Res<AssetServer>,
    audio_players: Query<&PlaybackSettings>,
) {
    if audio_players.iter().count() > 10 {
        sound_events.clear();
        return;
    }

    for event in sound_events.read() {
        let path: &'static str = match event {
            SoundEvent::Select => asset_paths::sounds::BEEP,
            SoundEvent::MovePiece => asset_paths::sounds::BEEP,
            SoundEvent::MoveMenu => asset_paths::sounds::BEEP,
            SoundEvent::Backspace => asset_paths::sounds::CAPTURE,
            SoundEvent::KeyInput => asset_paths::sounds::BEEP,
            SoundEvent::GameOverWin(team) => match team {
                PlayerTeam::White => asset_paths::sounds::BLACK_CHECKMATE,
                PlayerTeam::Black => asset_paths::sounds::WHITE_CHECKMATE,
            },
            SoundEvent::GameOverDraw => asset_paths::sounds::STALEMATE,
            SoundEvent::Error => asset_paths::sounds::ERROR,
            SoundEvent::Check => asset_paths::sounds::CHECK,
            SoundEvent::CapturePiece => asset_paths::sounds::CAPTURE,
        };

        let volume: f32 = match event {
            SoundEvent::MovePiece => 0.5,
            _ => 1.,
        };

        commands.spawn(AudioBundle {
            source: asset_server.load(path),
            settings: PlaybackSettings::DESPAWN
                .with_volume(Volume::Relative(VolumeLevel::new(volume))),
            ..default()
        });
    }
}
