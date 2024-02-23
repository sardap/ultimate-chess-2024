use std::collections::HashMap;

use bevy::{asset::RecursiveDependencyLoadState, prelude::*};
use bevy_common_assets::csv::{CsvAssetPlugin, LoadedCsv};

use crate::{local_input::AlgebraicMoveHistory, uchess::StateRefreshEvent, GameState};

pub struct OpeningsPlugin;

impl Plugin for OpeningsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CsvAssetPlugin::<OpeningRaw>::new(&["openings.tsv"]).with_delimiter(b'\t'));

        app.add_systems(Startup, setup);

        app.add_systems(OnEnter(GameState::Playing), setup_playing);

        app.add_systems(
            Update,
            (
                load_openings.run_if(in_state(GameState::LoadingOpenings)),
                analyze_state.run_if(in_state(GameState::Playing)),
            ),
        );
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let openings = OpeningHandle(asset_server.load("openings.openings.tsv"));
    commands.insert_resource(openings);
}

fn setup_playing(mut commands: Commands) {
    commands.remove_resource::<MatchedOpenings>();
    commands.insert_resource(MatchedOpenings::default());
}

#[derive(Debug, Clone)]
pub struct Opening {
    pub eco: String,
    pub name: String,
    pub moves: String,
}

#[derive(Resource)]
pub struct Openings {
    pub raw: Vec<Opening>,
    pub t_map: ternary_tree::Tst<Opening>,
    pub hash_map: HashMap<String, Opening>,
}

impl Default for Openings {
    fn default() -> Self {
        Self {
            raw: Vec::default(),
            t_map: ternary_tree::Tst::new(),
            hash_map: HashMap::default(),
        }
    }
}

fn load_openings(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    opening_handle: Res<OpeningHandle>,
    openings: Res<Assets<OpeningRaw>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if asset_server.get_recursive_dependency_load_state(&opening_handle.0)
        != Some(RecursiveDependencyLoadState::Loaded)
    {
        return;
    }

    let mut openings_parsed = Openings::default();

    for (_, opening_raw) in openings.iter() {
        let moves = opening_raw
            .pgn
            .split_whitespace()
            .map(|s| s.to_string())
            .filter(|s| !s.contains('.'))
            .collect::<Vec<_>>()
            .join(" ");

        let parsed = Opening {
            eco: opening_raw.eco.clone(),
            name: opening_raw.name.clone(),
            moves,
        };

        openings_parsed.t_map.insert(&parsed.moves, parsed.clone());

        openings_parsed
            .hash_map
            .insert(parsed.moves.clone(), parsed.clone());

        openings_parsed.raw.push(parsed);
    }

    commands.insert_resource(openings_parsed);

    game_state.set(GameState::LoadingAiPlayers);
}

#[derive(Resource)]
struct OpeningHandle(Handle<LoadedCsv<OpeningRaw>>);

#[derive(serde::Deserialize, Asset, TypePath, Debug)]
pub struct OpeningRaw {
    pub eco: String,
    pub name: String,
    pub pgn: String,
}

#[derive(Debug, Clone, Resource, Default)]
pub struct MatchedOpenings {
    pub matched_opening: Option<Opening>,
    pub next_openings: Vec<Opening>,
    pub longest_name_index: usize,
}

fn analyze_state(
    openings: Res<Openings>,
    move_history: Res<AlgebraicMoveHistory>,
    mut matched_openings: ResMut<MatchedOpenings>,
    mut state_refresh_reader: EventReader<StateRefreshEvent>,
) {
    if state_refresh_reader.is_empty() {
        return;
    }
    state_refresh_reader.clear();

    let move_str = move_history
        .moves
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    matched_openings.next_openings.clear();

    if move_str.len() == 0 {
        return;
    }

    openings.t_map.visit_complete_values(&move_str, |opening| {
        matched_openings.next_openings.push(opening.clone());
    });

    // Sort by edit distance
    matched_openings.next_openings.sort_by(|a, b| {
        let a_dist = strsim::levenshtein(&a.moves, &move_str);
        let b_dist = strsim::levenshtein(&b.moves, &move_str);

        a_dist.cmp(&b_dist)
    });

    // Only keep top 10
    matched_openings.next_openings.truncate(10);

    if let Some(matched_opening) = openings.hash_map.get(&move_str) {
        matched_openings.matched_opening = Some(matched_opening.clone());
    }
}
