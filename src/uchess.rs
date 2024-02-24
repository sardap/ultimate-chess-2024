use bevy::{
    audio::{Volume, VolumeLevel},
    prelude::*,
};
use chess::{BitBoard, Board, BoardBuilder, ChessMove, File, MoveGen, Piece, Square};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, str::FromStr};
use strum::EnumIter;

use crate::{asset_paths, local_input::AlgebraicMoveHistory, menu::Changeable, sounds, GameState};

pub struct ChessPlugin;

impl Plugin for ChessPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), setup_playing);
        app.add_systems(
            OnExit(GameState::Playing),
            (print_game_result, teardown_playing),
        );
        app.add_systems(
            Update,
            ((apply_move, update_state, check_game_over).chain())
                .run_if(in_state(GameState::Playing)),
        );

        app.add_systems(OnEnter(GameState::GameOver), setup_game_over);
        app.add_systems(OnExit(GameState::GameOver), teardown_game_over);
        app.add_systems(
            Update,
            game_over_input.run_if(in_state(GameState::GameOver)),
        );

        app.add_event::<MoveEvent>();
        app.add_event::<StateRefreshEvent>();
    }
}

#[derive(Debug, Event)]
pub struct StateRefreshEvent;

fn apply_move(
    mut commands: Commands,
    active_players: Query<(Entity, &PlayerTeam), (With<Player>, With<PlayerActive>)>,
    inactive_players: Query<(Entity, &PlayerTeam), (With<Player>, Without<PlayerActive>)>,
    mut chess_state: ResMut<ChessState>,
    mut piece_move_event_reader: EventReader<MoveEvent>,
    mut refresh_writer: EventWriter<StateRefreshEvent>,
    mut sound_event_writer: EventWriter<sounds::SoundEvent>,
) {
    for event in piece_move_event_reader.read() {
        let (a_id, a_t) = active_players.single();
        let (i_id, _) = inactive_players.single();

        let side_to_move: PlayerTeam = chess_state.current_position.side_to_move().into();
        if side_to_move != *a_t {
            error!("WARNING SOMEONE HAS SENT A MOVE WHILE NOT THERE TURN");
            continue;
        }

        let capture_made = chess_state
            .current_position
            .piece_on(event.mov.get_dest())
            .is_some();

        chess_state.apply_move(event.mov);

        sound_event_writer.send(if capture_made {
            sounds::SoundEvent::CapturePiece
        } else {
            sounds::SoundEvent::MovePiece
        });

        // Swap the active and inactive players
        commands.entity(a_id).remove::<PlayerActive>();
        commands.entity(i_id).insert(PlayerActive);

        refresh_writer.send(StateRefreshEvent);
    }

    piece_move_event_reader.clear()
}

#[derive(Debug, Clone, Component)]
struct BackgroundGameMusic;

#[derive(Debug, Clone, Copy, Component, Default)]
pub struct Player;

#[derive(Debug, Clone, Copy, Bundle, Default)]
pub struct PlayerBundle {
    pub team: PlayerTeam,
    pub player: Player,
}

#[derive(Debug, Clone, Copy, Default, EnumIter, PartialEq, Eq, Hash)]
pub enum ChessVariant {
    #[default]
    Standard,
    Chess960(u32),
    Horde,
    Horsies,
    Kawns,
    MidBattle,
}

impl ChessVariant {
    fn create_board(self) -> Board {
        match self {
            ChessVariant::Standard => Board::default(),
            ChessVariant::Chess960(val) => {
                // make i32 into seed
                let mut seed: [u8; 32] = [0; 32];
                for (i, b) in val.to_be_bytes().iter().enumerate() {
                    seed[i] = *b;
                }

                let mut rng = StdRng::from_seed(seed);
                create_chess_960_board(&mut rng)
            }
            ChessVariant::Horde => create_horde_board(Piece::Pawn),
            ChessVariant::Horsies => create_horsies_board(),
            ChessVariant::Kawns => create_knights_instead_of_pawns(),
            ChessVariant::MidBattle => create_mid_battle(),
        }
    }

    pub fn menu_string(&self) -> String {
        match self {
            ChessVariant::Standard => "Standard",
            ChessVariant::Chess960(_) => "Chess960",
            ChessVariant::Horde => "Horde",
            ChessVariant::Horsies => "Horsies",
            ChessVariant::Kawns => "Kawns",
            ChessVariant::MidBattle => "Mid Bat",
        }
        .to_owned()
    }
}

impl Changeable for ChessVariant {
    fn change(self, delta: i32) -> Self {
        use strum::IntoEnumIterator;

        let options = Self::iter().collect::<Vec<_>>();
        let index = options
            .iter()
            .position(|&option| match (self, option) {
                (ChessVariant::Chess960(_), ChessVariant::Chess960(_)) => true,
                _ => option == self,
            })
            .expect("Current enum variant not found in iterator");

        let options_len = options.len() as i32;
        let new_index = ((index as i32 + delta) % options_len + options_len) % options_len;

        let result = options[new_index as usize];

        match result {
            ChessVariant::Chess960(_) => {
                let mut rng = rand::thread_rng();
                ChessVariant::Chess960(rng.gen())
            }
            _ => result,
        }
    }
}

impl ToString for ChessVariant {
    fn to_string(&self) -> String {
        match self {
            ChessVariant::Chess960(seed) => format!("Chess960({})", seed),
            _ => self.menu_string(),
        }
        .to_owned()
    }
}

impl<'de> Deserialize<'de> for ChessVariant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if let Some(captures) = regex::Regex::new(r"^Chess960\((\d+)\)$")
            .unwrap()
            .captures(&s)
        {
            if let Ok(seed) = u32::from_str(&captures[1]) {
                return Ok(ChessVariant::Chess960(seed));
            }
        }

        match s.as_str() {
            "Standard" => Ok(ChessVariant::Standard),
            "Horde" => Ok(ChessVariant::Horde),
            "Horsies" => Ok(ChessVariant::Horsies),
            "Kawns" => Ok(ChessVariant::Kawns),
            _ => Err(serde::de::Error::custom("Invalid ChessVariant")),
        }
    }
}

// Serialization can remain automatic.
impl Serialize for ChessVariant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Copy, Resource)]
pub struct PlayOptions {
    pub chess_variant: ChessVariant,
}

fn setup_playing(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut refresh_writer: EventWriter<StateRefreshEvent>,
    players: Query<(Entity, &PlayerTeam)>,
    play_options: Option<Res<PlayOptions>>,
) {
    // Remove active player from all players
    for (entity, _) in &players {
        commands.entity(entity).remove::<PlayerActive>();
    }

    let white_player = players
        .iter()
        .find(|(_, team)| **team == PlayerTeam::White)
        .unwrap()
        .0;
    commands.entity(white_player).insert(PlayerActive);

    commands.remove_resource::<ChessState>();
    let variant = match play_options {
        Some(play_options) => play_options.chess_variant,
        None => ChessVariant::Standard,
    };
    commands.insert_resource(ChessState::new(variant));
    commands.remove_resource::<PlayOptions>();

    commands.remove_resource::<AlgebraicMoves>();
    commands.insert_resource(AlgebraicMoves::default());

    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::GAME),
            settings: PlaybackSettings::LOOP.with_volume(Volume::Relative(VolumeLevel::new(0.4))),
        },
        BackgroundGameMusic,
    ));

    refresh_writer.send(StateRefreshEvent);
}

fn print_game_result(move_history: Res<AlgebraicMoveHistory>) {
    let mut moves = move_history.moves.iter();

    let mut text = String::new();
    for i in 0..move_history.moves.len() / 2 {
        text.push_str(&format!("{}.", i + 1));
        if let Some(mov) = moves.next() {
            text.push_str(&format!(" {}", mov));
        }
        if let Some(mov) = moves.next() {
            text.push_str(&format!(" {}", mov));
        }
        text += " ";
    }

    debug!(text);
}

fn teardown_playing(
    mut commands: Commands,
    music: Query<Entity, With<BackgroundGameMusic>>,
    player_entities: Query<Entity, With<Player>>,
) {
    for entity in music.iter() {
        commands.entity(entity).despawn_recursive();
    }

    for entity in player_entities.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

#[derive(Debug, Clone, Resource)]
pub struct GameOver {
    pub end_type: EndType,
}

#[derive(Debug, Clone, Component)]
pub struct GameOverMusic;

fn setup_game_over(
    mut commands: Commands,
    chess_state: Res<ChessState>,
    asset_server: Res<AssetServer>,
    mut sound_event_writer: EventWriter<sounds::SoundEvent>,
) {
    let end_type = chess_state.game_over().unwrap();

    commands.insert_resource(GameOver { end_type });

    match end_type {
        EndType::Checkmate(team) => sound_event_writer.send(sounds::SoundEvent::GameOverWin(team)),
        EndType::Draw(_) => sound_event_writer.send(sounds::SoundEvent::GameOverDraw),
    }

    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::ENDGAME),
            settings: PlaybackSettings::LOOP,
        },
        GameOverMusic,
    ));
}

fn teardown_game_over(mut commands: Commands, despawn_query: Query<Entity, With<GameOverMusic>>) {
    for entity in despawn_query.iter() {
        commands.entity(entity).despawn_recursive();
    }

    commands.remove_resource::<GameOver>();
}

fn game_over_input(
    keyboard_input: Res<Input<KeyCode>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if keyboard_input.just_pressed(KeyCode::A) {
        game_state.set(GameState::Menu);
    }
}

fn update_state(
    mut algebraic_moves: ResMut<AlgebraicMoves>,
    mut refresh_reader: EventReader<StateRefreshEvent>,
    chess_state: Res<ChessState>,
) {
    if refresh_reader.is_empty() {
        return;
    }
    refresh_reader.clear();

    *algebraic_moves = chess_state.generate_algebraic_moves();
}

fn check_game_over(
    mut game_state: ResMut<NextState<GameState>>,
    mut refresh_reader: EventReader<StateRefreshEvent>,
    mut sound_event_writer: EventWriter<sounds::SoundEvent>,
    cur_state: Res<ChessState>,
) {
    if refresh_reader.is_empty() {
        return;
    }
    refresh_reader.clear();

    if cur_state.game_over().is_some() {
        game_state.set(GameState::GameOver);
    } else {
        for team in &[PlayerTeam::White, PlayerTeam::Black] {
            if cur_state.in_check(*team) {
                sound_event_writer.send(sounds::SoundEvent::Check);
            }
        }
    }
}

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerTeam {
    White,
    Black,
}

impl PlayerTeam {
    pub fn other(&self) -> Self {
        match self {
            PlayerTeam::White => PlayerTeam::Black,
            PlayerTeam::Black => PlayerTeam::White,
        }
    }
}

impl Default for PlayerTeam {
    fn default() -> Self {
        PlayerTeam::White
    }
}

impl ToString for PlayerTeam {
    fn to_string(&self) -> String {
        match self {
            PlayerTeam::White => "White",
            PlayerTeam::Black => "Black",
        }
        .to_owned()
    }
}

impl From<chess::Color> for PlayerTeam {
    fn from(value: chess::Color) -> Self {
        match value {
            chess::Color::White => PlayerTeam::White,
            chess::Color::Black => PlayerTeam::Black,
        }
    }
}

impl Into<chess::Color> for PlayerTeam {
    fn into(self) -> chess::Color {
        match self {
            PlayerTeam::White => chess::Color::White,
            PlayerTeam::Black => chess::Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct PlayerActive;

#[derive(Debug, Default, Component, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

pub fn piece_symbol_ascii(piece: Piece, _color: chess::Color) -> char {
    match piece {
        Piece::Pawn => 'P',
        Piece::Rook => 'R',
        Piece::Knight => 'N',
        Piece::Bishop => 'B',
        Piece::Queen => 'Q',
        Piece::King => 'K',
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceStandardMove {
    pub target: IVec2,
    pub capture_possible: bool,
    pub capture_target: Option<Entity>,
    check_causes_check: bool,
    pub causes_check: bool,
    pub promotion: Option<Piece>,
    pub algebraic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveHistory {
    pub mov: ChessMove,
}

#[derive(Debug, Clone, Resource)]
pub struct ChessState {
    current_position: chess::Board,
    fen: String,
    actions: Vec<MoveHistory>,
}

impl ChessState {
    pub fn new(variant: ChessVariant) -> Self {
        let board = variant.create_board();

        let mut result = Self {
            current_position: board,
            fen: String::new(),
            actions: Vec::new(),
        };

        result.refresh();

        result
    }

    pub fn refresh(&mut self) {
        self.populate_fen();
    }

    fn check_insufficient_material(&self) -> bool {
        self.current_position.pieces(Piece::Pawn).popcnt() == 0
            && self.current_position.pieces(Piece::Rook).popcnt() == 0
            && self.current_position.pieces(Piece::Queen).popcnt() == 0
            && ((self.current_position.pieces(Piece::Bishop).popcnt() < 2
                && self.current_position.pieces(Piece::Knight).popcnt() == 0)
                || (self.current_position.pieces(Piece::Knight).popcnt() < 2
                    && self.current_position.pieces(Piece::Bishop).popcnt() == 0))
    }

    pub fn game_over(&self) -> Option<EndType> {
        match self.current_position.status() {
            chess::BoardStatus::Ongoing => {
                if self.check_insufficient_material() {
                    Some(EndType::Draw(DrawReason::InsufficientMaterial))
                } else {
                    None
                }
            }
            chess::BoardStatus::Stalemate => Some(EndType::Draw(DrawReason::Stalemate)),
            chess::BoardStatus::Checkmate => {
                let current_turn: PlayerTeam = self.current_position.side_to_move().into();
                Some(EndType::Checkmate(current_turn.other()))
            }
        }
    }

    pub fn in_check(&self, team: PlayerTeam) -> bool {
        let color: chess::Color = team.into();
        let bitboard =
            self.current_position.checkers() & self.current_position.color_combined(!color);

        bitboard != chess::EMPTY
    }

    pub fn get_board(&self) -> &chess::Board {
        &self.current_position
    }

    pub fn half_move_count(&self) -> u16 {
        self.actions.len() as u16
    }

    pub fn apply_move(&mut self, mov: ChessMove) {
        self.actions.push(MoveHistory { mov });
        self.current_position = self.current_position.make_move_new(mov);
        self.refresh();
    }

    pub fn get_last_move(&self) -> Option<&ChessMove> {
        self.actions.last().map(|action| &action.mov)
    }

    pub fn get_last_move_for_player(&self, team: PlayerTeam) -> Option<&ChessMove> {
        let color: chess::Color = team.into();
        self.actions.iter().rev().find_map(|action| {
            if self.current_position.color_on(action.mov.get_dest()) == Some(color) {
                Some(&action.mov)
            } else {
                None
            }
        })
    }

    fn populate_fen(&mut self) {
        // 1. Generate the piece placement part
        self.fen = generate_fen(&self.current_position);
    }

    pub fn get_fen(&self) -> &str {
        &self.fen
    }

    fn generate_algebraic_moves(&self) -> AlgebraicMoves {
        let mut result = AlgebraicMoves::default();

        let mut iterable = MoveGen::new_legal(&self.current_position);

        for mov in &mut iterable {
            if let Some((team, san)) = chess_move_to_san(&self.current_position, &mov) {
                let move_dict = result.moves.get_mut(&team).unwrap();
                move_dict.insert(san, mov);
            }
        }

        result
    }
}

#[derive(Debug, Clone, Resource)]
pub struct AlgebraicMoves {
    pub moves: HashMap<PlayerTeam, HashMap<String, chess::ChessMove>>,
}

impl Default for AlgebraicMoves {
    fn default() -> Self {
        let mut result = Self {
            moves: HashMap::new(),
        };

        result.moves.insert(PlayerTeam::White, HashMap::new());
        result.moves.insert(PlayerTeam::Black, HashMap::new());

        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawReason {
    Stalemate,
    InsufficientMaterial,
    #[allow(dead_code)]
    ThreefoldRepetition,
    #[allow(dead_code)]
    FiftyMoveRule,
}

impl ToString for DrawReason {
    fn to_string(&self) -> String {
        match self {
            DrawReason::Stalemate => "Stalemate",
            DrawReason::InsufficientMaterial => "Material",
            DrawReason::ThreefoldRepetition => "Repetition",
            DrawReason::FiftyMoveRule => "50 Move",
        }
        .to_owned()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EndType {
    Checkmate(PlayerTeam),
    Draw(DrawReason),
}

pub fn generate_fen(board: &chess::Board) -> String {
    // 1. Generate the piece placement part
    let mut out_board: [[char; 8]; 8] = [[' '; 8]; 8];
    for rank in chess::ALL_RANKS {
        for file in chess::ALL_FILES {
            let square = Square::make_square(rank, file);
            if let Some(piece) = board.piece_on(square) {
                let mut symbol = match piece {
                    Piece::Pawn => 'P',
                    Piece::Rook => 'R',
                    Piece::Knight => 'N',
                    Piece::Bishop => 'B',
                    Piece::Queen => 'Q',
                    Piece::King => 'K',
                };

                let color = board.color_on(square).unwrap();

                if color == chess::Color::Black {
                    symbol = symbol.to_ascii_lowercase();
                };

                out_board[rank.to_index()][file.to_index()] = symbol;
            }
        }
    }

    // Compress board potions
    let mut fen = String::new();
    for rank in chess::ALL_RANKS {
        let mut rank_str = String::new();
        let mut count = 0;
        for file in chess::ALL_FILES {
            let element = out_board[rank.to_index()][file.to_index()];
            // count every consecutive empty square
            if element == ' ' {
                count += 1;
            } else {
                if count > 0 {
                    rank_str.push_str(count.to_string().as_str());
                    count = 0;
                }
                rank_str.push(element);
            }
        }
        if count > 0 {
            rank_str.push_str(count.to_string().as_str());
        }
        fen = format!("{}{}", rank_str, fen);
        if rank != chess::Rank::Eighth {
            fen = format!("/{}", fen);
        }
    }

    return fen;
}

fn file_to_string(file: File) -> &'static str {
    match file {
        File::A => "a",
        File::B => "b",
        File::C => "c",
        File::D => "d",
        File::E => "e",
        File::F => "f",
        File::G => "g",
        File::H => "h",
    }
}

fn rank_to_string(rank: chess::Rank) -> &'static str {
    match rank {
        chess::Rank::First => "1",
        chess::Rank::Second => "2",
        chess::Rank::Third => "3",
        chess::Rank::Fourth => "4",
        chess::Rank::Fifth => "5",
        chess::Rank::Sixth => "6",
        chess::Rank::Seventh => "7",
        chess::Rank::Eighth => "8",
    }
}

fn piece_to_string(piece: Piece) -> &'static str {
    match piece {
        Piece::Pawn => "",
        Piece::Rook => "R",
        Piece::Knight => "N",
        Piece::Bishop => "B",
        Piece::Queen => "Q",
        Piece::King => "K",
    }
}

fn chess_move_to_san(board: &chess::Board, chess_move: &ChessMove) -> Option<(PlayerTeam, String)> {
    if let Some(piece) = board.piece_on(chess_move.get_source()) {
        // 0. Check for castle
        {
            let castling_rights = board.castle_rights(board.side_to_move());
            if matches!(piece, Piece::King) && castling_rights != chess::CastleRights::NoRights {
                let queen_side_castle = matches!(
                    castling_rights,
                    chess::CastleRights::Both | chess::CastleRights::QueenSide
                );
                let king_side_castle = matches!(
                    castling_rights,
                    chess::CastleRights::Both | chess::CastleRights::KingSide
                );

                if chess_move.get_source().get_file() == chess::File::E
                    && chess_move.get_dest().get_file() == chess::File::G
                    && king_side_castle
                {
                    return Some((board.side_to_move().into(), "O-O".to_string()));
                } else if chess_move.get_source().get_file() == chess::File::E
                    && chess_move.get_dest().get_file() == chess::File::C
                    && queen_side_castle
                {
                    return Some((board.side_to_move().into(), "O-O-O".to_string()));
                }
            }
        }

        // 1. Add prefix
        let prefix = piece_to_string(piece);

        let mut notation = String::new();

        // 2. Check for capture
        if board.piece_on(chess_move.get_dest()).is_some() {
            if matches!(piece, Piece::Pawn) {
                notation.insert_str(0, file_to_string(chess_move.get_source().get_file()));
                notation.push_str("x");
            } else {
                notation.push_str("x");
            }
        }

        // 3. add target
        notation.push_str(file_to_string(chess_move.get_dest().get_file()));
        notation.push_str(rank_to_string(chess_move.get_dest().get_rank()));

        // 4. Check if rank and file are needed
        if !matches!(piece, Piece::Pawn) {
            // Check for other moves which can move to the same target
            let mut iterable = MoveGen::new_legal(&board);
            iterable.set_iterator_mask(BitBoard::from_square(chess_move.get_dest()));

            let move_source = chess_move.get_source();

            // Find worst case
            for other_moves in iterable {
                let other_piece = board.piece_on(other_moves.get_source()).unwrap();
                let other_source = other_moves.get_source();
                if other_source == move_source || other_piece != piece {
                    continue;
                }

                let mut extra = file_to_string(move_source.get_file()).to_owned();

                if other_source.get_file() == move_source.get_file() {
                    extra = format!("{}{}", extra, rank_to_string(move_source.get_rank()));
                }

                notation.insert_str(0, &extra);
            }
        }

        // 4. Add promotion
        if let Some(promo) = chess_move.get_promotion() {
            notation.push_str("=");
            notation.push_str(piece_to_string(promo));
        }

        // 5. add check / mate suffix
        {
            let color = board.side_to_move();
            let updated_board = board.make_move_new(*chess_move);
            // Is it mate?
            if MoveGen::new_legal(&updated_board).next().is_none() {
                notation.push_str("#");
            } else {
                let checkers = updated_board.checkers() & updated_board.color_combined(color);
                for checker in checkers {
                    if checker == chess_move.get_dest() {
                        notation.push_str("+");
                        break;
                    }
                }
            }
        }

        // 7. Add prefix
        notation.insert_str(0, prefix);

        let piece_team = board.color_on(chess_move.get_source()).unwrap();
        return Some((piece_team.into(), notation));
    }

    return None;
}

#[derive(Debug, Clone, Event, PartialEq, Eq)]
pub struct MoveEvent {
    pub mov: chess::ChessMove,
}

impl MoveEvent {
    pub fn new(mov: chess::ChessMove) -> Self {
        Self { mov }
    }
}

pub fn square_location(square: Square) -> IVec2 {
    IVec2::new(
        match square.get_file() {
            chess::File::A => 1,
            chess::File::B => 2,
            chess::File::C => 3,
            chess::File::D => 4,
            chess::File::E => 5,
            chess::File::F => 6,
            chess::File::G => 7,
            chess::File::H => 8,
        },
        match square.get_rank() {
            chess::Rank::First => 1,
            chess::Rank::Second => 2,
            chess::Rank::Third => 3,
            chess::Rank::Fourth => 4,
            chess::Rank::Fifth => 5,
            chess::Rank::Sixth => 6,
            chess::Rank::Seventh => 7,
            chess::Rank::Eighth => 8,
        },
    )
}

fn create_chess_960_board<T: Rng>(rng: &mut T) -> Board {
    use chess::Rank;

    let mut back_rank = [
        Piece::Rook,
        Piece::Knight,
        Piece::Bishop,
        Piece::Queen,
        Piece::King,
        Piece::Bishop,
        Piece::Knight,
        Piece::Rook,
    ];

    loop {
        back_rank.shuffle(rng);

        if is_valid_chess960(&back_rank) {
            break;
        }
    }

    let mut board_builder = BoardBuilder::new();
    board_builder.castle_rights(chess::Color::White, chess::CastleRights::Both);
    board_builder.castle_rights(chess::Color::Black, chess::CastleRights::Both);

    // Create a board and place pieces
    for (i, &piece) in back_rank.iter().enumerate() {
        board_builder.piece(
            Square::make_square(Rank::First, File::from_index(i)),
            piece,
            chess::Color::White,
        );
        board_builder.piece(
            Square::make_square(Rank::Eighth, File::from_index(i)),
            piece,
            chess::Color::Black,
        );
    }

    // Spawn pawns
    for file in chess::ALL_FILES {
        board_builder.piece(
            Square::make_square(Rank::Second, file),
            Piece::Pawn,
            chess::Color::White,
        );
        board_builder.piece(
            Square::make_square(Rank::Seventh, file),
            Piece::Pawn,
            chess::Color::Black,
        );
    }

    let board: Board = match board_builder.try_into() {
        Ok(board) => board,
        Err(_) => {
            board_builder.castle_rights(chess::Color::White, chess::CastleRights::NoRights);
            board_builder.castle_rights(chess::Color::Black, chess::CastleRights::NoRights);
            board_builder.try_into().unwrap()
        }
    };

    board
}

fn is_valid_chess960(back_rank: &[Piece; 8]) -> bool {
    let (mut white_bishop, mut black_bishop) = (false, false);
    let (mut king_found, mut first_rook_found, mut second_rook_found) = (false, false, false);

    for (i, &piece) in back_rank.iter().enumerate() {
        match piece {
            Piece::Bishop => {
                // Check for bishops on opposite colors
                if i % 2 == 0 {
                    white_bishop = true;
                } else {
                    black_bishop = true;
                }
            }
            Piece::King => {
                // The king should be placed after the first rook and before the second
                if first_rook_found && !second_rook_found {
                    king_found = true;
                }
            }
            Piece::Rook => {
                if !first_rook_found {
                    first_rook_found = true;
                } else {
                    second_rook_found = true;
                }
            }
            _ => {}
        }
    }

    white_bishop && black_bishop && king_found && first_rook_found && second_rook_found
}

fn create_horde_board(horde_piece: Piece) -> Board {
    let mut board_builder = BoardBuilder::new();
    board_builder.castle_rights(chess::Color::Black, chess::CastleRights::Both);
    board_builder.castle_rights(chess::Color::White, chess::CastleRights::NoRights);

    // Setup Black
    let black_pieces = [
        Piece::Rook,
        Piece::Knight,
        Piece::Bishop,
        Piece::Queen,
        Piece::King,
        Piece::Bishop,
        Piece::Knight,
        Piece::Rook,
    ];
    for (i, &piece) in black_pieces.iter().enumerate() {
        board_builder.piece(
            Square::make_square(chess::Rank::Eighth, File::from_index(i)),
            piece,
            chess::Color::Black,
        );
    }
    for file in chess::ALL_FILES {
        board_builder.piece(
            Square::make_square(chess::Rank::Seventh, file),
            Piece::Pawn,
            chess::Color::Black,
        );
    }

    // Setup White
    for rank in 0..4 {
        for file in chess::ALL_FILES {
            if rank == 0 && file == File::E {
                board_builder.piece(
                    Square::make_square(chess::Rank::First, file),
                    Piece::King,
                    chess::Color::White,
                );
            } else {
                board_builder.piece(
                    Square::make_square(chess::Rank::from_index(rank), file),
                    horde_piece,
                    chess::Color::White,
                );
            }
        }
    }
    for file in [File::B, File::C, File::F, File::G] {
        board_builder.piece(
            Square::make_square(chess::Rank::Fifth, file),
            Piece::Pawn,
            chess::Color::White,
        );
    }

    let board: Board = match board_builder.try_into() {
        Ok(board) => board,
        Err(err) => panic!("Error creating board: {}", err),
    };

    board
}

fn create_horsies_board() -> Board {
    let mut board_builder = BoardBuilder::new();

    let back_rank_pieces = [
        Piece::Knight,
        Piece::Knight,
        Piece::Knight,
        Piece::Knight,
        Piece::King,
        Piece::Knight,
        Piece::Knight,
        Piece::Knight,
    ];
    for (i, &piece) in back_rank_pieces.iter().enumerate() {
        let file = File::from_index(i);
        board_builder.piece(
            Square::make_square(chess::Rank::Eighth, file),
            piece,
            chess::Color::Black,
        );
        board_builder.piece(
            Square::make_square(chess::Rank::First, file),
            piece,
            chess::Color::White,
        );
    }

    // Spawn pawns
    for (rank, color) in [
        (chess::Rank::Seventh, chess::Color::Black),
        (chess::Rank::Second, chess::Color::White),
    ] {
        for file in chess::ALL_FILES {
            board_builder.piece(Square::make_square(rank, file), Piece::Pawn, color);
        }
    }

    let board: Board = match board_builder.try_into() {
        Ok(board) => board,
        Err(err) => panic!("Error creating board: {}", err),
    };

    board
}

fn create_knights_instead_of_pawns() -> Board {
    let mut board_builder = BoardBuilder::new();

    // Setup Black
    let back_rank_pieces = [
        Piece::Rook,
        Piece::Bishop,
        Piece::Bishop,
        Piece::Queen,
        Piece::King,
        Piece::Bishop,
        Piece::Bishop,
        Piece::Rook,
    ];
    for (i, &piece) in back_rank_pieces.iter().enumerate() {
        let file = File::from_index(i);
        board_builder.piece(
            Square::make_square(chess::Rank::Eighth, file),
            piece,
            chess::Color::Black,
        );
        board_builder.piece(
            Square::make_square(chess::Rank::First, file),
            piece,
            chess::Color::White,
        );
    }

    // Spawn knights
    for (rank, color) in [
        (chess::Rank::Seventh, chess::Color::Black),
        (chess::Rank::Second, chess::Color::White),
    ] {
        for file in chess::ALL_FILES {
            board_builder.piece(Square::make_square(rank, file), Piece::Knight, color);
        }
    }

    let board: Board = match board_builder.try_into() {
        Ok(board) => board,
        Err(err) => panic!("Error creating board: {}", err),
    };

    board
}

fn create_mid_battle() -> Board {
    let mut board_builder = BoardBuilder::new();

    let piece_locations = [
        (Piece::Pawn, Square::from_str("a1").unwrap()),
        (Piece::Pawn, Square::from_str("a3").unwrap()),
        (Piece::Pawn, Square::from_str("b4").unwrap()),
        (Piece::Pawn, Square::from_str("c4").unwrap()),
        (Piece::Pawn, Square::from_str("d4").unwrap()),
        (Piece::Pawn, Square::from_str("e4").unwrap()),
        (Piece::Pawn, Square::from_str("f4").unwrap()),
        (Piece::Pawn, Square::from_str("g4").unwrap()),
        (Piece::Pawn, Square::from_str("h3").unwrap()),
        (Piece::Pawn, Square::from_str("h1").unwrap()),
        (Piece::Rook, Square::from_str("c1").unwrap()),
        (Piece::Rook, Square::from_str("f1").unwrap()),
        (Piece::King, Square::from_str("d1").unwrap()),
        (Piece::Queen, Square::from_str("e1").unwrap()),
        (Piece::Bishop, Square::from_str("f3").unwrap()),
        (Piece::Bishop, Square::from_str("c3").unwrap()),
    ];

    for (piece, square) in &piece_locations {
        board_builder.piece(*square, *piece, chess::Color::White);
        // opposite side for black
        let rank =
            chess::Rank::from_index(chess::Rank::Eighth.to_index() - square.get_rank().to_index());
        board_builder.piece(
            Square::make_square(rank, square.get_file()),
            *piece,
            chess::Color::Black,
        );
    }

    let board: Board = match board_builder.try_into() {
        Ok(board) => board,
        Err(err) => panic!("Error creating board: {}", err),
    };

    board
}

lazy_static! {
    static ref ZOBRIST_TABLE: Vec<Vec<u64>> = {
        let mut rng = rand::thread_rng();
        let mut table = Vec::new();
        for _ in 0..64 {
            let mut piece_table = Vec::new();
            for _ in 0..6 {
                piece_table.push(rng.gen());
            }
            table.push(piece_table);
        }
        table
    };
}

const ZOBRIST_TABLE_BLACK_TO_MOVE: u64 = 12738094573457687482;

pub fn hash_board(board: &Board) -> u64 {
    let mut hash = 0;
    if board.side_to_move() == chess::Color::Black {
        hash ^= ZOBRIST_TABLE_BLACK_TO_MOVE;
    }

    for square in chess::ALL_SQUARES {
        if let Some(piece) = board.piece_on(square) {
            hash ^= ZOBRIST_TABLE[square.to_index()][piece.to_index()];
        }
    }
    hash
}
