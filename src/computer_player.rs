use std::{collections::HashMap, time::Duration};

#[allow(unused_imports)]
use crate::{
    asset_paths,
    evaluation::{
        blunder_score, nega_max_alpha_beta, EvaluationPresets, GamePhase, BEST_PIECE_SQUARE_PHASES,
    },
    local_input::{AlgebraicNotationInputEvent, LocalPlayerInput},
    menu::Changeable,
    sounds::SoundEvent,
    transposition_table::{TranspositionTable, TranspositionTableTrait},
    uchess::{
        AlgebraicMoves, ChessState, ChessVariant, PlayOptions, PlayerActive, PlayerBundle,
        PlayerTeam,
    },
    GameState,
};
use base64::prelude::*;
use bevy::{
    asset::RecursiveDependencyLoadState, prelude::*, tasks::AsyncComputeTaskPool,
    utils::hashbrown::HashSet,
};
use bevy_async_task::{AsyncReceiver, AsyncTask};
use bevy_common_assets::json::JsonAssetPlugin;
use bevy_prng::ChaCha8Rng;
use bevy_rand::prelude::*;
use chess::{Board, ChessMove, Piece};
use rand::{prelude::IteratorRandom, seq::SliceRandom, Rng};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use strum::EnumIter;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_mt::{prelude::*, utils::console_ln, Thread};
use weighted_rand::builder::*;

pub struct ComputerPlyerPlugin;

impl Plugin for ComputerPlyerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsonAssetPlugin::<PlayerAIGroup>::new(&["computer.json"]));

        app.add_systems(Startup, setup);
        app.add_systems(
            Update,
            load_ai_player_group.run_if(in_state(GameState::LoadingAiPlayers)),
        );

        app.add_systems(OnEnter(GameState::ComputerPlay), setup_computer_play);
        app.add_systems(OnExit(GameState::ComputerPlay), teardown_computer_menu);
        app.add_systems(OnExit(GameState::Playing), teardown_playing);

        app.add_systems(
            Update,
            (
                clear_computer_player_next_move,
                (
                    process_bogo_move_computer_turn,
                    process_bogo_piece_computer_turn,
                    process_profile_computer_turn,
                    process_blundy_computer_turn,
                ),
                send_computer_player_turn,
            )
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
        app.add_systems(Update, (process_delayed_computer_turn,));

        app.add_systems(
            Update,
            process_menu_input.run_if(in_state(GameState::ComputerPlay)),
        );
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let ai_group_handle = PlayerAIGroupHandle(asset_server.load("player_profiles.computer.json"));
    commands.insert_resource(ai_group_handle);
}

fn load_ai_player_group(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player_ai_handle: Res<PlayerAIGroupHandle>,
    player_ai_group: Res<Assets<PlayerAIGroup>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if asset_server.get_recursive_dependency_load_state(&player_ai_handle.0)
        != Some(RecursiveDependencyLoadState::Loaded)
    {
        return;
    }

    for player_ai_group in player_ai_group.iter() {
        let mut player_ai_group = player_ai_group.1.clone();

        for (name, profile) in player_ai_group.profiles.iter_mut() {
            if name == "Not Idiot" {
                profile.piece_square_phases = BEST_PIECE_SQUARE_PHASES.clone();
            }

            profile.evaluation_presets = Some(EvaluationPresets::new(&profile));
        }

        commands.insert_resource(player_ai_group);
    }

    game_state.set(GameState::Menu);
}

#[derive(Debug, Default, Component)]
struct ComputerMenuMusic;

fn setup_computer_play(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ComputerMenu::default());

    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::MULTIPLAYER_MENU),
            settings: PlaybackSettings::LOOP,
            ..default()
        },
        ComputerMenuMusic,
    ));
}

fn teardown_computer_menu(
    mut commands: Commands,
    despawn_query: Query<Entity, With<ComputerMenuMusic>>,
) {
    commands.remove_resource::<ComputerMenu>();

    for entity in despawn_query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

fn teardown_playing() {}

#[derive(Debug, Default, Component)]
pub struct ComputerPlayer {
    pub cool_down: Timer,
    pub next_move: Option<String>,
    pub sent_move: bool,
}

impl ComputerPlayer {
    pub fn new() -> Self {
        Self {
            cool_down: Timer::from_seconds(1.0, TimerMode::Once),
            next_move: None,
            sent_move: false,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy, EnumIter)]
pub enum ComputerType {
    #[default]
    None,
    Paul,
    Mango,
    NotIdiot,
    BogoMove,
    BogoPiece,
    Blundy,
}

impl ToString for ComputerType {
    fn to_string(&self) -> String {
        match self {
            ComputerType::None => "Human",
            ComputerType::BogoMove => "Bogo M",
            ComputerType::BogoPiece => "Bogo P",
            ComputerType::Paul => "Paul",
            ComputerType::NotIdiot => "NotIdiot",
            ComputerType::Mango => "Mango",
            ComputerType::Blundy => "Blundy",
        }
        .to_string()
    }
}

impl Changeable for ComputerType {}

#[derive(Debug, Default, Component)]
pub struct BogoMoveComputer;

#[derive(Debug, Default, Component)]
pub struct BogoPieceComputer;

#[derive(Debug, Default, Component)]
pub struct BlundyComputer;

#[derive(Debug, Component)]
pub struct ProfileComputer {
    pub name: String,
    pub evaluation_presets: EvaluationPresets,
    pub table: Arc<Mutex<TranspositionTable>>,
}

impl ProfileComputer {
    pub fn new(profile: &str, ai_group: &PlayerAIGroup) -> Self {
        let eval_presets = ai_group
            .get_profile(profile)
            .unwrap()
            .evaluation_presets
            .clone()
            .unwrap();

        Self {
            name: profile.to_string(),
            evaluation_presets: eval_presets,
            table: Arc::new(Mutex::new(TranspositionTable::default())),
        }
    }
}

#[derive(Debug, Default, EnumIter, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ComputerMenuOption {
    #[default]
    White,
    Black,
    ChessVariant,
}

impl Changeable for ComputerMenuOption {}

#[derive(Debug, Default, Resource)]
pub struct ComputerMenu {
    pub white: ComputerType,
    pub black: ComputerType,
    pub chess_variant: ChessVariant,
    pub selected: ComputerMenuOption,
}

fn process_menu_input(
    mut commands: Commands,
    mut computer_menu: ResMut<ComputerMenu>,
    mut sound_event: EventWriter<SoundEvent>,
    mut game_state: ResMut<NextState<GameState>>,
    keyboard_input: Res<Input<KeyCode>>,
    player_ai_group: Res<PlayerAIGroup>,
) {
    if keyboard_input.just_pressed(KeyCode::Up) {
        computer_menu.selected = computer_menu.selected.change(-1);
        sound_event.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Down) {
        computer_menu.selected = computer_menu.selected.change(1);
        sound_event.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
        sound_event.send(SoundEvent::Error);
    }

    match computer_menu.selected {
        ComputerMenuOption::White | ComputerMenuOption::Black => {
            let active = match computer_menu.selected {
                ComputerMenuOption::White => &mut computer_menu.white,
                ComputerMenuOption::Black => &mut computer_menu.black,
                _ => unreachable!(),
            };

            if keyboard_input.just_pressed(KeyCode::Left) {
                *active = active.change(-1);
                sound_event.send(SoundEvent::MoveMenu);
            }

            if keyboard_input.just_pressed(KeyCode::Right) {
                *active = active.change(1);
                sound_event.send(SoundEvent::MoveMenu);
            }
        }
        ComputerMenuOption::ChessVariant => {
            if keyboard_input.just_pressed(KeyCode::Left) {
                computer_menu.chess_variant = computer_menu.chess_variant.change(-1);
                sound_event.send(SoundEvent::MoveMenu);
            }

            if keyboard_input.just_pressed(KeyCode::Right) {
                computer_menu.chess_variant = computer_menu.chess_variant.change(1);
                sound_event.send(SoundEvent::MoveMenu);
            }
        }
    }

    if keyboard_input.just_pressed(KeyCode::Return) {
        let mut create_player = |player_team: PlayerTeam, computer_type: ComputerType| {
            let mut player = commands.spawn(PlayerBundle {
                team: player_team,
                ..default()
            });
            if computer_type != ComputerType::None {
                player.insert((
                    ComputerPlayer::new(),
                    EntropyComponent::<ChaCha8Rng>::default(),
                ));
            }
            match computer_type {
                ComputerType::BogoMove => {
                    player.insert(BogoMoveComputer::default());
                }
                ComputerType::BogoPiece => {
                    player.insert(BogoPieceComputer::default());
                }
                ComputerType::Paul => {
                    player.insert(ProfileComputer::new("Paul", &player_ai_group));
                }
                ComputerType::NotIdiot => {
                    player.insert(ProfileComputer::new("Not Idiot", &player_ai_group));
                }
                ComputerType::Mango => {
                    player.insert(ProfileComputer::new("Mango", &player_ai_group));
                }
                ComputerType::Blundy => {
                    player.insert(BlundyComputer::default());
                }
                ComputerType::None => {
                    player.insert(LocalPlayerInput);
                }
            };
        };

        create_player(PlayerTeam::White, computer_menu.white);
        create_player(PlayerTeam::Black, computer_menu.black);

        commands.insert_resource(PlayOptions {
            chess_variant: computer_menu.chess_variant,
        });

        game_state.set(GameState::Playing);
        sound_event.send(SoundEvent::MoveMenu);
    }
}

fn send_computer_player_turn(
    mut an_input_writer: EventWriter<AlgebraicNotationInputEvent>,
    mut player_inputs: Query<(&PlayerTeam, &mut ComputerPlayer), With<PlayerActive>>,
    time: Res<Time>,
) {
    for (team, mut computer_player) in player_inputs.iter_mut() {
        computer_player.cool_down.tick(time.delta());

        if !computer_player.sent_move
            && computer_player.cool_down.finished()
            && computer_player.next_move.is_some()
        {
            let input_str = computer_player.next_move.clone().unwrap();
            an_input_writer.send(AlgebraicNotationInputEvent::new(input_str.clone(), *team));
            computer_player.sent_move = true;
            continue;
        }
    }
}

fn clear_computer_player_next_move(
    mut removed: RemovedComponents<PlayerActive>,
    mut player_inputs: Query<&mut ComputerPlayer>,
) {
    for entity in removed.read() {
        if let Ok(mut computer_player) = player_inputs.get_mut(entity) {
            computer_player.next_move = None;
            computer_player.sent_move = false;
            computer_player.cool_down.reset();
        }
    }
}

fn process_bogo_move_computer_turn(
    mut player_inputs: Query<
        (
            &PlayerTeam,
            &mut ComputerPlayer,
            &mut EntropyComponent<ChaCha8Rng>,
        ),
        (With<BogoMoveComputer>, With<PlayerActive>),
    >,
    algebraic_moves: Res<AlgebraicMoves>,
) {
    for (team, mut computer_player, mut rng) in player_inputs.iter_mut() {
        if computer_player.next_move.is_some() {
            continue;
        }

        let input_str = match algebraic_moves.moves.get(team) {
            Some(team_moves) => team_moves.keys().choose(&mut rng.fork_rng()),
            None => None,
        };

        computer_player.next_move = input_str.cloned();
    }
}

fn process_bogo_piece_computer_turn(
    mut player_inputs: Query<
        (
            &PlayerTeam,
            &mut ComputerPlayer,
            &mut EntropyComponent<ChaCha8Rng>,
        ),
        (With<BogoPieceComputer>, With<PlayerActive>),
    >,
    chess_state: Res<ChessState>,
    algebraic_moves: Res<AlgebraicMoves>,
) {
    for (team, mut computer_player, mut rng) in player_inputs.iter_mut() {
        if computer_player.next_move.is_some() {
            continue;
        }

        let team_moves = algebraic_moves.moves.get(team).unwrap();

        if team_moves.len() == 0 {
            continue;
        }

        let mut move_table = HashMap::new();
        for piece in chess::ALL_PIECES {
            move_table.insert(piece, Vec::new());
        }

        for (san, mov) in team_moves {
            if let Some(piece) = chess_state.get_board().piece_on(mov.get_source()) {
                move_table.get_mut(&piece).unwrap().push(san);
            }
        }

        // Remove all pieces with no moves
        move_table.retain(|_, v| v.len() > 0);

        let mut rng = rng.fork_rng();

        let selected_piece = move_table.values().choose(&mut rng).unwrap();
        let selected_move = selected_piece
            .into_iter()
            .choose(&mut rng)
            .unwrap()
            .to_string();

        computer_player.next_move = Some(selected_move);
    }
}

fn process_blundy_computer_turn(
    mut player_inputs: Query<
        (
            &PlayerTeam,
            &mut ComputerPlayer,
            &mut EntropyComponent<ChaCha8Rng>,
        ),
        (With<BlundyComputer>, With<PlayerActive>),
    >,
    chess_state: Res<ChessState>,
    algebraic_moves: Res<AlgebraicMoves>,
) {
    for (team, mut computer_player, mut rng) in player_inputs.iter_mut() {
        if computer_player.next_move.is_some() {
            continue;
        }

        let possible_moves = match algebraic_moves.moves.get(team) {
            Some(team_moves) => team_moves,
            None => continue,
        };

        if possible_moves.len() == 0 {
            continue;
        }

        let mut rng = rng.fork_rng();

        if chess_state.half_move_count() == 0 {
            computer_player.next_move = Some("f3".to_string());
        } else if chess_state.half_move_count() < 4 {
            let selected_move = possible_moves.iter().choose(&mut rng).unwrap().0.clone();
            computer_player.next_move = Some(selected_move);
            continue;
        }

        let mut worst_score = f32::MAX;
        let mut worst_moves = Vec::new();
        let board = chess_state.get_board();

        for (algebraic_notation, chess_move) in possible_moves {
            // Don't move knights
            if chess_state.get_board().piece_on(chess_move.get_source()) == Some(Piece::Knight) {
                continue;
            }
            let updated_board = board.make_move_new(*chess_move);
            let score = -blunder_score(&updated_board, 1);

            if score == worst_score {
                worst_moves.push(algebraic_notation);
            } else if score < worst_score {
                worst_score = score;
                worst_moves.clear();
                worst_moves.push(algebraic_notation);
            }
        }

        let selected_move = if worst_moves.len() == 0 {
            possible_moves.iter().choose(&mut rng).unwrap().0.clone()
        } else {
            worst_moves.iter().choose(&mut rng).unwrap().to_string()
        };

        computer_player.next_move = Some(selected_move);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerAITeamProfile {
    positions: HashMap<String, HashMap<String, i32>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerAIThinkingDepth {
    pub levels: Vec<i32>,
    pub move_hit: [f32; 6],
    pub thinking_time: [f32; 2],
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct PieceSquareTables {
    pub pawn: Vec<f32>,
    pub knight: Vec<f32>,
    pub bishop: Vec<f32>,
    pub rook: Vec<f32>,
    pub queen: Vec<f32>,
    pub king: Vec<f32>,
}

impl PieceSquareTables {
    pub fn get_square_table(&self, piece: Piece) -> &[f32] {
        match piece {
            Piece::Pawn => &self.pawn,
            Piece::Knight => &self.knight,
            Piece::Bishop => &self.bishop,
            Piece::Rook => &self.rook,
            Piece::Queen => &self.queen,
            Piece::King => &self.king,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct PieceSquarePhases {
    pub opening: PieceSquareTables,
    pub middle_game: PieceSquareTables,
    pub end_game: PieceSquareTables,
}

impl PieceSquarePhases {
    pub fn get_square_table(&self, phase: GamePhase, piece: Piece) -> &[f32] {
        let table = match phase {
            GamePhase::Opening => &self.opening,
            GamePhase::MiddleGame => &self.middle_game,
            GamePhase::EndGame => &self.end_game,
        };

        table.get_square_table(piece)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct PlayerAIProfile {
    pub white: PlayerAITeamProfile,
    pub black: PlayerAITeamProfile,
    pub depth: PlayerAIThinkingDepth,
    pub piece_weights: [f32; 6],
    pub piece_square_phases: PieceSquarePhases,
    pub check_bonus: f32,
    pub decision_algorithm: String,
    #[serde(skip)]
    pub evaluation_presets: Option<EvaluationPresets>,
}

#[derive(serde::Deserialize, Asset, TypePath, Debug, Resource, Clone)]
pub struct PlayerAIGroup {
    profiles: HashMap<String, PlayerAIProfile>,
}

impl PlayerAIGroup {
    pub fn get_profile(&self, name: &str) -> Option<&PlayerAIProfile> {
        self.profiles.get(name)
    }
}

#[derive(Resource)]
struct PlayerAIGroupHandle(Handle<PlayerAIGroup>);

fn pgn_parser_hash(s: &str) -> String {
    let mut h: u32 = 0;
    for &byte in s.as_bytes() {
        h = h.wrapping_add(byte as u32);
        h = h.wrapping_add(h << 10);
        h = h ^ (h >> 6);
    }

    h = h.wrapping_add(h << 3);
    h = h ^ (h >> 11);
    h = h.wrapping_add(h << 15);

    let bytes: [u8; 4] = h.to_be_bytes();
    let b64 = BASE64_STANDARD.encode(&bytes);
    b64[..5].to_string()
}

fn process_profile_computer_turn(
    mut commands: Commands,
    mut player_inputs: Query<
        (
            Entity,
            &PlayerTeam,
            &mut ComputerPlayer,
            &ProfileComputer,
            &mut EntropyComponent<ChaCha8Rng>,
        ),
        (With<PlayerActive>, Without<DelayedTurnEvaluation>),
    >,
    player_ai_group: Res<PlayerAIGroup>,
    algebraic_moves: Res<AlgebraicMoves>,
    chess_state: Res<ChessState>,
) {
    let position_hash = pgn_parser_hash(&chess_state.get_fen());

    for (entity, team, mut computer_player, com_profile, mut rng) in player_inputs.iter_mut() {
        if computer_player.next_move.is_some() {
            continue;
        }

        let possible_moves = match algebraic_moves.moves.get(team) {
            Some(team_moves) => team_moves,
            None => continue,
        };

        if possible_moves.len() == 0 {
            continue;
        }

        if possible_moves.len() == 1 {
            computer_player.next_move = Some(possible_moves.keys().next().unwrap().clone());
            continue;
        }

        let profile_name = &com_profile.name;

        let profile = match player_ai_group.get_profile(&profile_name) {
            Some(profile) => profile,
            None => continue,
        };

        let team_profile = match team {
            PlayerTeam::White => &profile.white,
            PlayerTeam::Black => &profile.black,
        };

        debug!(
            "color: {} player: {}, turn: {} hash: {} fen: {}",
            team.to_string(),
            &profile_name,
            chess_state.half_move_count() / 2 + 1,
            position_hash,
            chess_state.get_fen()
        );

        // Get all known moves from current game state
        if let Some(position_moves) = team_profile.positions.get(&position_hash) {
            let mut moves = Vec::new();
            let mut weights = Vec::new();
            // We should be able to do any move from this position but :shurg:
            for possible_move in possible_moves.keys() {
                if let Some(weight) = position_moves.get(possible_move) {
                    moves.push(possible_move);
                    weights.push(*weight as f32);
                }
            }

            if moves.len() > 0 {
                let builder = WalkerTableBuilder::new(&weights);
                let wa_table = builder.build();

                let index = wa_table.next();
                computer_player.next_move = Some(moves[index].clone());
                continue;
            }
        }

        // Unmatched game state use eval + piece preference

        // Remove last move form possible moves
        let last_move = chess_state.get_last_move_for_player(*team);

        let possible_moves = if possible_moves.len() > 0 {
            possible_moves
                .into_iter()
                .filter(|(_, mov)| Some(*mov) != last_move)
                .map(|(san, mov)| (san.clone(), *mov))
                .collect::<HashMap<_, _>>()
        } else {
            possible_moves.clone()
        };

        let evaluation_presets = com_profile.evaluation_presets.clone();

        let task = AsyncTask::new(delayed_turn_eval(
            rng.fork_rng(),
            entity,
            *chess_state.get_board(),
            evaluation_presets,
            possible_moves,
            Arc::clone(&com_profile.table),
            chess_state.half_move_count(),
        ));

        let (fut, rx) = task.into_parts();

        let task_pool = AsyncComputeTaskPool::get();
        let task = task_pool.spawn(fut);
        task.detach();

        commands.entity(entity).insert(DelayedTurnEvaluation { rx });

        debug!("unknown state found, generated task",);
    }
}

#[derive(Component)]
struct DelayedTurnEvaluation {
    pub rx: AsyncReceiver<DelayedMoveEval>,
}

#[derive(Debug, Clone)]
struct DelayedMoveEval {
    selected_move: String,
}

fn process_delayed_computer_turn(
    mut commands: Commands,
    chess_state: Option<Res<ChessState>>,
    mut computer_players: Query<(
        Entity,
        &mut ComputerPlayer,
        &ProfileComputer,
        &mut DelayedTurnEvaluation,
    )>,
) {
    if chess_state.is_none() {
        return;
    }

    for (entity, mut computer_player, profile, mut delayed) in computer_players.iter_mut() {
        if let Some(eval) = delayed.rx.try_recv() {
            debug!("Got computer applying delayed eval move");

            computer_player.next_move = Some(eval.selected_move);

            debug!(
                "new transpose table size {}",
                profile.table.lock().unwrap().size()
            );

            commands.entity(entity).remove::<DelayedTurnEvaluation>();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
lazy_static! {
    static ref SEM_PERMITS: usize = 1.max((num_cpus::get() as f32 * 0.85) as usize);
}

lazy_static! {
    static ref SCORE_IMPROVEMENT_THRESHOLD: i64 = {
        let val: f32 = 20. * 10000.;

        val.round() as i64
    };
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Date)]
    fn now() -> f64;
}

fn current_time() -> Duration {
    #[cfg(target_arch = "wasm32")]
    {
        Duration::from_micros((now() * 1000.) as u64)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::UNIX_EPOCH;

        let unix_secs = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        Duration::from_nanos(unix_secs as u64)
    }
}

const MIN_DEPTH: i32 = 0;
#[cfg(not(target_arch = "wasm32"))]
const MAX_DEPTH: i32 = 100;

#[cfg(target_arch = "wasm32")]
#[derive(Serialize, Deserialize)]
struct WasmWorkerOutput {
    move_scores: HashMap<i32, Vec<MoveScore>>,
}

#[cfg(target_arch = "wasm32")]
fn calculate_move_scores(
    moves: &HashMap<String, ChessMove>,
    board: &Board,
    starting_score: i64,
    depth: i32,
    half_move_count: u16,
    transposition_table: &mut TranspositionTable,
    evaluation_preset: &EvaluationPresets,
    end_time: Duration,
) -> Vec<MoveScore> {
    let mut next_scores = Vec::new();

    for (san, chess_move) in moves {
        let piece = board.piece_on(chess_move.get_source()).unwrap();

        let score = if san.contains("#") {
            i64::MAX
        } else {
            let updated_board: Board = board.make_move_new(*chess_move);
            -nega_max_alpha_beta(
                transposition_table,
                evaluation_preset,
                &updated_board,
                depth + 1,
                half_move_count + 1,
            )
        };

        let score = score.min(js_sys::Number::MAX_SAFE_INTEGER as i64);
        let score = score.max(js_sys::Number::MIN_SAFE_INTEGER as i64);

        next_scores.push(MoveScore {
            san: san.clone(),
            piece,
            score,
        });

        if current_time() > end_time && depth > 0 {
            console_ln!("Max move time reached breaking out of move loop");
            return Vec::new();
        }

        if depth > 1 && score > starting_score + *SCORE_IMPROVEMENT_THRESHOLD {
            console_ln!("Stopping early due score threshold being reached");
            break;
        }
    }

    next_scores
}

#[cfg(target_arch = "wasm32")]
async fn wasm_move_eval_worker(
    th: &Thread,
    evaluation_preset: EvaluationPresets,
    board: Board,
    moves: HashMap<String, ChessMove>,
    half_move_count: u16,
    max_depth: i32,
    end_time: std::time::Duration,
) -> Result<WasmWorkerOutput, JsValue> {
    debug!("Starting wasm move eval worker");

    #[derive(Serialize, Deserialize)]
    struct WasmThreadOutput {
        move_scores: HashMap<i32, Vec<MoveScore>>,
    }

    let ans = exec!(th, move || {
        let mut full_eval_scores = HashMap::new();
        let mut transposition_table = TranspositionTable::default();

        let starting_score = nega_max_alpha_beta(
            &mut transposition_table,
            &evaluation_preset,
            &board,
            0,
            half_move_count,
        );

        for depth in MIN_DEPTH..(max_depth + 1) {
            console_ln!("Running depth {}", depth);

            let scores = calculate_move_scores(
                &moves,
                &board,
                starting_score,
                depth,
                half_move_count,
                &mut transposition_table,
                &evaluation_preset,
                end_time,
            );

            if scores.len() > 0 {
                let best_score = scores
                    .iter()
                    .max_by(|a, b| a.score.cmp(&b.score))
                    .unwrap()
                    .score;

                full_eval_scores.insert(depth, scores);

                if current_time() > end_time
                    || (depth > 1 && best_score > starting_score + *SCORE_IMPROVEMENT_THRESHOLD)
                {
                    console_ln!("Stopping early due to best score");
                    break;
                }
            } else {
                break;
            }
        }

        console_ln!(
            "Reached depth of {}",
            full_eval_scores.keys().max().unwrap()
        );

        let result = WasmThreadOutput {
            move_scores: full_eval_scores,
        };

        console_ln!("about to serialize");
        let result = match serde_wasm_bindgen::to_value(&result) {
            Ok(value) => value,
            Err(err) => {
                console_ln!("serialize error: {:?}", err);
                return Err(JsValue::from_str("serialize error"));
            }
        };
        console_ln!("serialize complete");

        Ok(result)
    })
    .await;

    match ans {
        Ok(ans) => {
            debug!("wasm move eval worker complete");
            let result: WasmThreadOutput = serde_wasm_bindgen::from_value(ans).unwrap();
            Ok(WasmWorkerOutput {
                move_scores: result.move_scores,
            })
        }
        Err(err) => {
            error!("wasm move eval worker error: {:?}", err);
            Err(err)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct MoveScore {
    san: String,
    piece: Piece,
    score: i64,
}

async fn delayed_turn_eval<T: Rng>(
    mut rng: T,
    source: Entity,
    board: Board,
    evaluation_preset: EvaluationPresets,
    moves: HashMap<String, chess::ChessMove>,
    #[allow(unused_variables, unused_mut)] mut transposition_table: Arc<Mutex<TranspositionTable>>,
    half_move_count: u16,
) -> DelayedMoveEval {
    let mut move_scores: HashMap<i32, Vec<MoveScore>>;

    let end_time = current_time() + evaluation_preset.get_thinking_duration(&mut rng);

    debug!(
        "Starting delayed turn eval for {:?} will run for {}",
        source.index(),
        (end_time - current_time()).as_secs_f32()
    );

    let random_depth = evaluation_preset.get_random_depth(&mut rng);

    #[cfg(target_arch = "wasm32")]
    {
        crate::wasm_thread::initialize_wasm_thread().await;

        // I give up
        unsafe {
            let holder = &crate::wasm_thread::WASM_THREAD_HOLDER.lock().unwrap();

            let th = &holder.as_ref().unwrap().thread;
            // Transpose table is ignored in wasm
            // We just can't send it and receive it form the worker
            // Without causing massive lag
            match wasm_move_eval_worker(
                th,
                evaluation_preset.clone(),
                board,
                moves.clone(),
                half_move_count,
                random_depth,
                end_time,
            )
            .await
            {
                Ok(output) => {
                    debug!("wasm_move_eval_worker success");
                    move_scores = output.move_scores;
                }
                Err(err) => {
                    error!("wasm_move_eval_worker error: {:?}", err);
                    move_scores = HashMap::new();
                }
            };
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        move_scores = HashMap::new();

        let sem = Arc::new(async_semaphore::Semaphore::new(*SEM_PERMITS));
        let should_stop = Arc::new(Mutex::new(false));

        let starting_score = nega_max_alpha_beta(
            transposition_table.clone(),
            &evaluation_preset,
            &board,
            0,
            half_move_count,
            should_stop.clone(),
        );

        for depth in MIN_DEPTH..(MAX_DEPTH + 1) {
            use std::sync::mpsc;
            use std::thread;

            struct Job {
                san: String,
                chess_move: ChessMove,
            }

            let jobs = moves
                .iter()
                .map(|(san, chess_move)| Job {
                    san: san.clone(),
                    chess_move: *chess_move,
                })
                .collect::<Vec<_>>();

            let (tx, rx) = mpsc::channel();

            let handles: Vec<_> = jobs
                .into_iter()
                .map(|job| {
                    let tx = tx.clone();
                    let board = board.clone();
                    let evaluation_preset = evaluation_preset.clone();
                    let transposition_table = Arc::clone(&transposition_table);
                    let should_stop = Arc::clone(&should_stop);
                    let sem = Arc::clone(&sem);

                    thread::spawn(move || {
                        let permit = loop {
                            match sem.try_acquire() {
                                Some(permit) => break permit,
                                None => {
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                            }
                        };

                        let piece = board.piece_on(job.chess_move.get_source()).unwrap();

                        let score: i64 = if job.san.contains("#") {
                            i64::MAX
                        } else {
                            let updated_board: Board = board.make_move_new(job.chess_move);
                            -nega_max_alpha_beta(
                                transposition_table,
                                &evaluation_preset,
                                &updated_board,
                                depth,
                                half_move_count,
                                should_stop,
                            )
                        };
                        drop(permit);

                        let _ = tx.send(MoveScore {
                            san: job.san,
                            piece,
                            score,
                        });
                    })
                })
                .collect();

            let mut results = Vec::new();

            while current_time() < end_time {
                match rx.try_recv() {
                    Ok(move_score) => {
                        results.push(move_score);
                        if results.len() == moves.len() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Still running sleep
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }

            let mut stop = current_time() > end_time;
            if !stop {
                for handle in handles {
                    handle.join().unwrap();
                }

                drop(tx);

                let best_score = results
                    .iter()
                    .max_by(|a, b| a.score.cmp(&b.score))
                    .unwrap()
                    .score;

                debug!(
                    "Depth {} all joined {} best score {} threshold {}",
                    depth,
                    moves.len(),
                    best_score,
                    starting_score + *SCORE_IMPROVEMENT_THRESHOLD
                );

                if depth > 1 && best_score > starting_score + *SCORE_IMPROVEMENT_THRESHOLD {
                    debug!("Stopping early due to best score");
                    stop = true;
                }

                move_scores.insert(depth, results);
            }

            if stop {
                // Stop all the running threads
                let mut should_stop = should_stop.lock().unwrap();
                *should_stop = true;
                break;
            }
        }

        transposition_table.trim(half_move_count);
    }

    let selected_move = {
        debug!("Evals {} complete", move_scores.len());

        if move_scores.len() == 0 {
            None
        } else {
            let depth = random_depth.min(*move_scores.keys().max().unwrap());

            let move_eval_depth = move_scores.get_mut(&depth).unwrap();

            debug!("Using depth: {} (Random depth: {})", depth, random_depth);

            // Filter moves by hit rate
            move_eval_depth
                .retain(|mov| rng.gen::<f32>() < evaluation_preset.move_hit[mov.piece.to_index()]);

            if move_eval_depth.len() == 0 {
                None
            } else {
                debug!("filtered by hit rate {} remaining", move_eval_depth.len());

                let mut scores = HashSet::new();
                for mov in move_eval_depth.iter() {
                    scores.insert(mov.score);
                }

                let mut scores = scores.into_iter().collect::<Vec<_>>();
                scores.sort();

                debug!("unique scores: {:?}", scores);

                // Get best score
                let best_score = *scores.last().unwrap();

                // Get all other moves with the same score
                let best_moves: Vec<_> = move_eval_depth
                    .iter()
                    .filter(|mov| mov.score == best_score)
                    .collect();

                debug!("best score: {} with moves {}", best_score, best_moves.len());

                Some(best_moves.choose(&mut rng).unwrap().san.clone())
            }
        }
    };

    let selected_move = match selected_move {
        Some(selected_move) => selected_move,
        None => moves.keys().choose(&mut rng).unwrap().clone(),
    };

    debug!(
        "Delayed eval turn selected move: {} for {}",
        selected_move,
        source.index(),
    );

    DelayedMoveEval {
        selected_move: selected_move.to_string(),
    }
}
