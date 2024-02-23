#![feature(async_closure)]
#![feature(trivial_bounds)]
mod asset_paths;
mod computer_player;
mod credits;
mod evaluation;
mod how_to_play;
mod local_input;
mod menu;
mod multiplayer;
mod openings;
mod render;
mod sounds;
mod transposition_table;
mod uchess;
#[cfg(target_arch = "wasm32")]
mod wasm_thread;

#[macro_use]
extern crate lazy_static;

use crate::{render::RenderPlugin, uchess::ChessPlugin};
use bevy::{asset::AssetMetaCheck, prelude::*, window::PresentMode};
use bevy_ascii_terminal::TerminalPlugin;
use bevy_mod_reqwest::ReqwestPlugin;
use bevy_prng::ChaCha8Rng;
use bevy_rand::prelude::*;
use computer_player::ComputerPlyerPlugin;
use credits::CreditsPlugin;
use how_to_play::HowToPlayPlugin;
use local_input::LocalInputPlugin;
use menu::MenuPlugin;
use multiplayer::MultiplayerPlugin;
use openings::OpeningsPlugin;
use sounds::SoundPlugin;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(States, Clone, Copy, Default, Eq, PartialEq, Hash, Debug)]
pub enum GameState {
    #[default]
    LoadingOpenings,
    LoadingAiPlayers,
    Menu,
    PlayLocal,
    Playing,
    GameOver,
    Credits,
    Multiplayer,
    ComputerPlay,
    HowToPlay,
}

pub fn build_out_app(app: &mut App) {
    app.add_state::<GameState>()
        .insert_resource(AssetMetaCheck::Never)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Ultimate Chess 2024".to_string(),
                resolution: (500., 650.).into(),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            TerminalPlugin,
            ReqwestPlugin,
            EntropyPlugin::<ChaCha8Rng>::default(),
        ))
        .add_plugins((
            MultiplayerPlugin,
            ComputerPlyerPlugin,
            CreditsPlugin,
            SoundPlugin,
            ChessPlugin,
            RenderPlugin,
            MenuPlugin,
            LocalInputPlugin,
            OpeningsPlugin,
            HowToPlayPlugin,
        ));
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn app() {
    let mut app = App::new();
    build_out_app(&mut app);
    app.run();
}
