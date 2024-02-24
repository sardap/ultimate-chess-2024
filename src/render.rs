use std::collections::HashSet;

use bevy::prelude::*;
use bevy_ascii_terminal::prelude::*;
use chess::{Board, ChessMove, Piece};
use strum::IntoEnumIterator;

use crate::{
    computer_player::{ComputerMenu, ComputerMenuOption},
    credits::{CreditText, Invisible},
    local_input::{AlgebraicNotationInput, LocalPlayerInput},
    menu::{MenuInput, MenuOptions},
    multiplayer::{
        ErrorMessage, HostMenu, HostMenuOptions, JoinInput, MultiplayerGameSession,
        MultiplayerMenuInput, MultiplayerOptions, MultiplayerState,
    },
    openings::MatchedOpenings,
    uchess::{
        piece_symbol_ascii, square_location, AlgebraicMoves, ChessState, EndType, GameOver,
        PlayerActive, PlayerTeam,
    },
    GameState,
};

#[derive(Debug)]
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        app.add_systems(
            Update,
            (
                scroll_text,
                render_playing.run_if(in_state(GameState::Playing)),
                render_game_over.run_if(in_state(GameState::GameOver)),
                render_menu.run_if(in_state(GameState::Menu)),
                render_credits.run_if(in_state(GameState::Credits)),
                render_computer_menu.run_if(in_state(GameState::ComputerPlay)),
                render_how_to_play.run_if(in_state(GameState::HowToPlay)),
                // Multiplayer
                (
                    render_multiplayer_menu.run_if(in_state(MultiplayerState::Menu)),
                    render_multiplayer_host_menu.run_if(in_state(MultiplayerState::HostMenu)),
                    render_multiplayer_host.run_if(in_state(MultiplayerState::HostSetup)),
                    render_multiplayer_host.run_if(in_state(MultiplayerState::HostWaiting)),
                    render_multiplayer_join.run_if(in_state(MultiplayerState::JoinInput)),
                    render_multiplayer_join.run_if(in_state(MultiplayerState::Join)),
                    render_multiplayer_error.run_if(in_state(MultiplayerState::Error)),
                ),
            ),
        );
    }
}

pub const STAGE_SIZE: IVec2 = IVec2::from_array([10, 13]);

fn setup(mut commands: Commands) {
    // Create the terminal
    let terminal = Terminal::new(STAGE_SIZE).with_border(Border::single_line());

    commands.spawn((
        // Spawn the terminal bundle from our terminal
        TerminalBundle::from(terminal),
        // Automatically set up the camera to render the terminal
        AutoCamera,
    ));

    commands.spawn((LongTextScroller::default(),));
}

#[derive(Debug, Clone, Component)]
pub struct LongTextScroller {
    next_tick: Timer,
    offset: usize,
    max_offset: usize,
}

impl Default for LongTextScroller {
    fn default() -> Self {
        Self {
            next_tick: Timer::from_seconds(0.5, TimerMode::Repeating),
            offset: 0,
            max_offset: 0,
        }
    }
}

impl LongTextScroller {
    pub fn new(text: &str) -> Self {
        let mut result = Self::default();
        result.set_text(text);
        result
    }

    pub fn set_text(&mut self, text: &str) {
        self.max_offset = text.len();
        self.offset = 0;
    }

    pub fn get_sub_str(&self, message: &str) -> String {
        let name_length = message.chars().count();
        let sub_string = (0..STAGE_SIZE.x as usize)
            .map(|n| {
                let char_index = (self.offset + n) % name_length;
                message.chars().nth(char_index).unwrap_or_default()
            })
            .collect::<String>();

        sub_string
    }

    pub fn reset(&mut self) {
        self.offset = 0;
    }
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn render_menu(mut terminal: Query<&mut Terminal>, menu_input: Query<&MenuInput>) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    let menu_input = menu_input.single();

    terminal.put_string([2, STAGE_SIZE.y - 2], "ULTIMATE".fg(Color::RED));
    terminal.put_string([5, STAGE_SIZE.y - 3], "CHESS".fg(Color::RED));
    terminal.put_string([6, STAGE_SIZE.y - 4], "2024".fg(Color::GOLD));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 6], Tile::from('~'));
    }

    for (i, option) in MenuOptions::iter().enumerate() {
        let mut tile = Tile::from('>');
        let color = if option == menu_input.selected {
            Color::WHITE
        } else {
            Color::GRAY
        };
        tile.fg_color = color;

        terminal.put_tile([0, STAGE_SIZE.y - 7 - i as i32], tile);
        terminal.put_string(
            [1, STAGE_SIZE.y - 7 - i as i32],
            option.to_string().fg(color),
        );
    }

    terminal.put_string(
        [STAGE_SIZE.x - (VERSION.len() + 1) as i32, 0],
        format!("v{}", VERSION).fg(Color::GREEN),
    )
}

fn chess_color_to_render_color(color: chess::Color) -> Color {
    match color {
        chess::Color::White => Color::rgb(0.761, 0.698, 0.502),
        chess::Color::Black => Color::BLACK,
    }
}

struct ColorSet {
    normal: Color,
    highlighted: Color,
    last_move: Color,
}

impl ColorSet {
    const fn new(normal: Color, highlighted: Color, last_move: Color) -> Self {
        Self {
            normal,
            highlighted,
            last_move,
        }
    }
}

const COLOR_WHITE: ColorSet =
    ColorSet::new(Color::BEIGE, Color::YELLOW, Color::rgb(1.0, 1.0, 0.56));
const COLOR_BLACK: ColorSet =
    ColorSet::new(Color::DARK_GREEN, Color::GREEN, Color::rgb(0.33, 1.0, 0.35));

fn render_board(
    terminal: &mut Terminal,
    board: &Board,
    highlighted_positions: &HashSet<IVec2>,
    last_move: Option<&ChessMove>,
) {
    for i in 0..8 {
        terminal.put_tile(
            [0, STAGE_SIZE.y - 1 - (8 - i as i32)],
            Tile::from(char::from(i + 48 + 1)),
        );
        terminal.put_tile(
            [1 + i as i32, STAGE_SIZE.y - 1],
            Tile::from(char::from(i + 65)),
        );
    }

    let get_tile_color = |x: i32, y: i32| {
        let transformed_position = IVec2::new(x, y);

        let set = if (x + y) % 2 == 0 {
            COLOR_BLACK
        } else {
            COLOR_WHITE
        };

        if highlighted_positions.contains(&transformed_position) {
            set.highlighted
        } else if match last_move {
            Some(last_move) => {
                let source = square_location(last_move.get_source());
                let dest = square_location(last_move.get_dest());
                source == transformed_position || dest == transformed_position
            }
            None => false,
        } {
            set.last_move
        } else {
            set.normal
        }
    };

    for i in 0..8 {
        for j in 0..8 {
            let mut tile = Tile::default();
            tile.bg_color = get_tile_color(i + 1, j + 1);
            terminal.put_tile([i + 1, j + 4], tile);
        }
    }

    for &color in &[chess::Color::White, chess::Color::Black] {
        for &piece in &chess::ALL_PIECES {
            let bitboard = board.color_combined(color) & board.pieces(piece);
            // Iterate over each square and check if a piece of the specified color and type is present
            for square in bitboard {
                let position = square_location(square);

                let mut tile = Tile::from(piece_symbol_ascii(piece, color));
                tile.fg_color = match piece {
                    Piece::King => {
                        let checked_bitboard = board.color_combined(color) & board.checkers();
                        let mut in_check = false;
                        for checked_square in checked_bitboard {
                            if checked_square == square {
                                in_check = true;
                            }
                        }

                        if in_check {
                            Color::RED
                        } else {
                            chess_color_to_render_color(color)
                        }
                    }
                    _ => chess_color_to_render_color(color),
                };

                tile.bg_color = get_tile_color(position.x, position.y);
                terminal.put_tile([position.x, position.y + 3], tile);
            }
        }
    }
}

struct DotCycle {
    dots: i8,
    timer: Timer,
}

impl DotCycle {
    fn tick(&mut self, time: &Time) {
        self.timer.tick(time.delta());
        if self.timer.just_finished() {
            self.dots += 1;
            if self.dots > 3 {
                self.dots = 0;
            }
        }
    }
}

impl Default for DotCycle {
    fn default() -> Self {
        Self {
            dots: 0,
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
        }
    }
}

fn render_playing(
    mut terminal: Query<&mut Terminal>,
    mut text_scroller: Query<&mut LongTextScroller>,
    mut dot_cycle: Local<DotCycle>,
    time: Res<Time>,
    chess_state: Res<ChessState>,
    matched_openings: Res<MatchedOpenings>,
    algebraic_moves: Res<AlgebraicMoves>,
    input: Query<&AlgebraicNotationInput>,
    player_inputs: Query<&PlayerTeam, (With<PlayerActive>, With<LocalPlayerInput>)>,
) {
    dot_cycle.tick(&time);

    let mut terminal = terminal.single_mut();
    terminal.clear();

    let input = input.single();

    let current_turn: PlayerTeam = chess_state.get_board().side_to_move().into();

    // Get highlighted Positions
    let mut highlighted_positions = HashSet::new();
    if let Some(possible_moves) = algebraic_moves.moves.get(&current_turn) {
        // Highlight complete move
        if let Some(chess_move) = possible_moves.get(&input.current_input) {
            // Highlight source piece
            highlighted_positions.insert(square_location(chess_move.get_source()));
            highlighted_positions.insert(square_location(chess_move.get_dest()));
        } else {
            for possible_move in &input.auto_complete {
                if let Some(chess_move) = possible_moves.get(possible_move) {
                    // Highlight source piece
                    highlighted_positions.insert(square_location(chess_move.get_source()));
                    highlighted_positions.insert(square_location(chess_move.get_dest()));
                }
            }
        }
    }

    render_board(
        &mut terminal,
        &chess_state.get_board(),
        &highlighted_positions,
        chess_state.get_last_move(),
    );

    terminal.put_string(
        [0, 3],
        format!("{} Go", current_turn.to_string().chars().nth(0).unwrap()),
    );

    // current team does not have local input
    if player_inputs.iter().len() == 1 {
        let input_str = format!(">{}", input.current_input);
        terminal.put_string([0, 2], input_str);
        terminal.put_string([0, 1], input.auto_complete.join(","));
    } else {
        let dots = ".".repeat(dot_cycle.dots as usize);
        terminal.put_string([0, 2], format!("Waiting{}", dots));
    }

    terminal.put_string(
        [0, 0],
        match &matched_openings.matched_opening {
            Some(opening) => {
                let mut text_scrolling: Mut<'_, LongTextScroller> = text_scroller.single_mut();

                let name = format!("{}   ", opening.name);

                if text_scrolling.max_offset != name.len() {
                    text_scrolling.set_text(&name);
                }

                text_scrolling.get_sub_str(&name).fg(Color::GREEN)
            }
            None => "No matches!".to_string().fg(Color::RED),
        },
    );
}

fn render_game_over(
    mut terminal: Query<&mut Terminal>,
    game_over: Res<GameOver>,
    chess_state: Res<ChessState>,
) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    let highlighted = HashSet::new();

    render_board(&mut terminal, chess_state.get_board(), &highlighted, None);

    match game_over.end_type {
        EndType::Checkmate(winner) => {
            terminal.put_string([0, 2], "CHECKMATE!");
            terminal.put_string([0, 1], format!("Win:{}", winner.to_string()));
        }
        EndType::Draw(reason) => {
            terminal.put_string([0, 2], "DRAW!");
            terminal.put_string([0, 1], reason.to_string());
        }
    }

    terminal.put_string([0, 0], "A TO AGAIN");

    return;
}

fn scroll_text(time: Res<Time>, mut text_scroller: Query<&mut LongTextScroller>) {
    for mut text_scroller in text_scroller.iter_mut() {
        text_scroller.next_tick.tick(time.delta());

        if text_scroller.next_tick.just_finished() {
            text_scroller.offset += 1;

            if text_scroller.offset >= text_scroller.max_offset {
                text_scroller.offset = 0;
            }
        }
    }
}

fn render_multiplayer_menu(
    mut terminal: Query<&mut Terminal>,
    menu_input: Query<&MultiplayerMenuInput>,
) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    let menu_input = menu_input.single();

    terminal.put_string([2, STAGE_SIZE.y - 3], "MULTIPLE".fg(Color::RED));
    terminal.put_string([3, STAGE_SIZE.y - 4], "PLAYERS".fg(Color::RED));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 7], Tile::from('~'));
    }

    for (i, option) in MultiplayerOptions::iter().enumerate() {
        let mut tile = Tile::from('>');
        let color = if option == menu_input.selected {
            Color::WHITE
        } else {
            Color::GRAY
        };
        tile.fg_color = color;

        terminal.put_tile([0, STAGE_SIZE.y - 9 - i as i32], tile);
        terminal.put_string(
            [1, STAGE_SIZE.y - 9 - i as i32],
            option.to_string().fg(color),
        );
    }
}

fn render_multiplayer_host_menu(mut terminal: Query<&mut Terminal>, menu_input: Query<&HostMenu>) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    let menu_input = menu_input.single();

    terminal.put_string([3, STAGE_SIZE.y - 3], "HOST".fg(Color::RED));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 7], Tile::from('~'));
    }

    let variant_row = STAGE_SIZE.y - 8;
    let start_row = STAGE_SIZE.y - 11;
    let back_row = STAGE_SIZE.y - 12;
    {
        let fg_color = if menu_input.selected == HostMenuOptions::ChessVariant {
            Color::WHITE
        } else {
            Color::GRAY
        };

        terminal.put_string(
            [STAGE_SIZE.x / 2 - 7 / 2, variant_row],
            "Variant".fg(fg_color),
        );
        terminal.put_string(
            [
                STAGE_SIZE.x / 2 - menu_input.chess_variant.menu_string().len() as i32 / 2,
                variant_row - 1,
            ],
            menu_input.chess_variant.menu_string().fg(fg_color),
        );
        terminal.put_char([0, variant_row - 1], '<'.fg(fg_color));
        terminal.put_char([STAGE_SIZE.x - 1, variant_row - 1], '>'.fg(fg_color));
    }

    {
        let fg_color = if menu_input.selected == HostMenuOptions::Start {
            Color::WHITE
        } else {
            Color::GRAY
        };

        terminal.put_string([0, start_row], ">Start".fg(fg_color));
    }

    {
        let fg_color = if menu_input.selected == HostMenuOptions::Back {
            Color::WHITE
        } else {
            Color::GRAY
        };

        terminal.put_string([0, back_row], ">Back".fg(fg_color));
    }
}

fn render_multiplayer_host(
    mut terminal: Query<&mut Terminal>,
    multiplayer_session: Option<Res<MultiplayerGameSession>>,
) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    terminal.put_string([3, STAGE_SIZE.y - 2], "HOST".fg(Color::RED));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 4], Tile::from('~'));
    }

    match multiplayer_session {
        Some(multiplayer_session) => {
            terminal.put_string(
                [2, STAGE_SIZE.y - 6],
                multiplayer_session.game_key.as_str().fg(Color::GREEN),
            );

            terminal.put_string([0, STAGE_SIZE.y - 9], "Waiting");
            terminal.put_string([0, STAGE_SIZE.y - 11], "For");
            terminal.put_string([0, STAGE_SIZE.y - 13], "Player...");
        }
        None => {
            terminal.put_string([0, STAGE_SIZE.y - 9], "Waiting");
            terminal.put_string([0, STAGE_SIZE.y - 11], "For");
            terminal.put_string([0, STAGE_SIZE.y - 13], "Server...");
        }
    }
}

fn render_multiplayer_join(mut terminal: Query<&mut Terminal>, join_input: Option<Res<JoinInput>>) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    terminal.put_string([3, STAGE_SIZE.y - 2], "JOIN".fg(Color::RED));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 5], Tile::from('~'));
    }

    match join_input {
        Some(join_input) => {
            terminal.put_string([0, STAGE_SIZE.y - 7], "Game ID");
            terminal.put_string([0, STAGE_SIZE.y - 8], format!(">{}", join_input.game_key));
        }
        None => {
            terminal.put_string([0, STAGE_SIZE.y - 9], "Waiting");
            terminal.put_string([0, STAGE_SIZE.y - 11], "For");
            terminal.put_string([0, STAGE_SIZE.y - 13], "Server...");
        }
    }
}

fn render_multiplayer_error(
    mut terminal: Query<&mut Terminal>,
    error: Query<&ErrorMessage>,
    mut text_scroller: Query<&mut LongTextScroller>,
) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    terminal.put_string([2, STAGE_SIZE.y - 2], "ERROR".fg(Color::RED));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 5], Tile::from('~'));
    }

    for (i, error) in error.iter().enumerate() {
        let mut text_scroller: Mut<'_, LongTextScroller> = text_scroller.single_mut();

        let name = format!("{}   ", error.message);

        if text_scroller.max_offset != name.len() {
            text_scroller.set_text(&name);
        }

        let message = text_scroller.get_sub_str(&name).fg(Color::RED);

        terminal.put_string([0, STAGE_SIZE.y - 7 - i as i32], message);
        break;
    }

    terminal.put_string([2, STAGE_SIZE.y - 10], "Press");
    terminal.put_string([3, STAGE_SIZE.y - 11], "Any");
    terminal.put_string([3, STAGE_SIZE.y - 12], "Key");
}

fn render_credits(
    mut terminal: Query<&mut Terminal>,
    credit_text: Query<(&CreditText, &LongTextScroller), (Without<Invisible>, With<CreditText>)>,
) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    terminal.put_string(
        [STAGE_SIZE.x - 7, STAGE_SIZE.y - 2],
        "CREDITS".fg(Color::RED),
    );

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 4], Tile::from('~'));
    }

    let mut y = 0;
    for (i, (text, text_scroller)) in credit_text.iter().enumerate() {
        let sub_str = text_scroller.get_sub_str(&text.text);

        for (j, letter) in sub_str.chars().enumerate() {
            static COLORS: [Color; 6] = [
                Color::RED,
                Color::ORANGE,
                Color::YELLOW,
                Color::GREEN,
                Color::BLUE,
                Color::PURPLE,
            ];

            let mut tile = Tile::from(letter);
            tile.fg_color = COLORS[(j + i) % 6];

            terminal.put_tile([j as i32, STAGE_SIZE.y - 6 - y as i32], tile);
        }

        y += 2;
    }
}

fn render_computer_menu(mut terminal: Query<&mut Terminal>, computer_menu: Res<ComputerMenu>) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    terminal.put_string([STAGE_SIZE.x - 2, STAGE_SIZE.y - 1], "VS".fg(Color::RED));
    terminal.put_string(
        [STAGE_SIZE.x - 8, STAGE_SIZE.y - 2],
        "COMPUTER".fg(Color::RED),
    );

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 4], Tile::from('~'));
    }

    for (i, (team, option)) in [
        (PlayerTeam::White, computer_menu.white),
        (PlayerTeam::Black, computer_menu.black),
    ]
    .iter()
    .enumerate()
    {
        let start_row = if i == 0 {
            STAGE_SIZE.y - 5
        } else {
            STAGE_SIZE.y - 8
        };

        let fg_color = match computer_menu.selected {
            ComputerMenuOption::White | ComputerMenuOption::Black => {
                if computer_menu.selected == ComputerMenuOption::White && *team == PlayerTeam::White
                    || computer_menu.selected == ComputerMenuOption::Black
                        && *team == PlayerTeam::Black
                {
                    Color::WHITE
                } else {
                    Color::GRAY
                }
            }
            ComputerMenuOption::ChessVariant => Color::GRAY,
        };

        terminal.put_string(
            [
                STAGE_SIZE.x / 2 - team.to_string().len() as i32 / 2,
                start_row,
            ],
            team.to_string().fg(fg_color),
        );
        terminal.put_string(
            [
                STAGE_SIZE.x / 2 - option.to_string().len() as i32 / 2,
                start_row - 1,
            ],
            option.to_string().fg(fg_color),
        );
        terminal.put_char([0, start_row - 1], '<'.fg(fg_color));
        terminal.put_char([STAGE_SIZE.x - 1, start_row - 1], '>'.fg(fg_color));
    }

    {
        let fg_color = if computer_menu.selected == ComputerMenuOption::ChessVariant {
            Color::WHITE
        } else {
            Color::GRAY
        };
        let row = STAGE_SIZE.y - 11;

        terminal.put_string([STAGE_SIZE.x / 2 - 7 / 2, row], "Variant".fg(fg_color));
        terminal.put_string(
            [
                STAGE_SIZE.x / 2 - computer_menu.chess_variant.menu_string().len() as i32 / 2,
                row - 1,
            ],
            computer_menu.chess_variant.menu_string().fg(fg_color),
        );
        terminal.put_char([0, row - 1], '<'.fg(fg_color));
        terminal.put_char([STAGE_SIZE.x - 1, row - 1], '>'.fg(fg_color));
    }
}

fn render_how_to_play(mut terminal: Query<&mut Terminal>) {
    let mut terminal = terminal.single_mut();
    terminal.clear();

    terminal.put_string([STAGE_SIZE.x - 3, STAGE_SIZE.y - 2], "HOW".fg(Color::RED));
    terminal.put_string([STAGE_SIZE.x - 5, STAGE_SIZE.y - 3], "PLAY?".fg(Color::RED));

    for i in 0..STAGE_SIZE.x {
        terminal.put_tile([i, STAGE_SIZE.y - 5], Tile::from('~'));
    }

    const HOW_TO_TEXT: [(&'static str, Color); 8] = [
        ("1.Move via", Color::AQUAMARINE),
        ("keyboard.", Color::AQUAMARINE),
        ("", Color::BLACK),
        ("2.Tab to", Color::TEAL),
        ("auto-comp.", Color::TEAL),
        ("", Color::BLACK),
        ("3.ESC to", Color::BLUE),
        ("go menu.", Color::BLUE),
    ];

    for (i, (text, color)) in HOW_TO_TEXT.iter().enumerate() {
        terminal.put_string(
            [0, STAGE_SIZE.y - 6 - i as i32],
            text.to_string().fg(*color),
        );
    }
}
