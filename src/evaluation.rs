use chess::{BitBoard, Board, MoveGen, Piece};
use rand::Rng;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};
use std::time::Duration;
use weighted_rand::{
    builder::{NewBuilder, WalkerTableBuilder},
    table::WalkerTable,
};

use crate::computer_player::{PieceSquarePhases, PieceSquareTables, PlayerAIProfile};
use crate::transposition_table::{
    SearchFlag, SearchResult, TranspositionTable, TranspositionTableTrait,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvaluationPresets {
    pub piece_weights: [f32; 6],
    pub piece_square_phases: PieceSquarePhases,
    pub move_hit: [f32; 6],
    pub depth_levels: Vec<i32>,
    pub check_bonus: f32,
    thinking_time: [f32; 2],
    depth_random_table: WalkerTable,
}

impl EvaluationPresets {
    pub fn new(profile: &PlayerAIProfile) -> Self {
        let mut weights = Vec::new();
        for weight in profile.depth.levels.iter() {
            weights.push(*weight as u32);
        }

        let wa_builder = WalkerTableBuilder::new(&weights);
        let depth_random_table = wa_builder.build();

        Self {
            piece_weights: profile.piece_weights.clone(),
            piece_square_phases: profile.piece_square_phases.clone(),
            move_hit: profile.depth.move_hit,
            depth_levels: profile.depth.levels.clone(),
            check_bonus: profile.check_bonus,
            thinking_time: profile.depth.thinking_time,
            depth_random_table,
        }
    }

    pub fn get_random_depth<T: Rng>(&self, rng: &mut T) -> i32 {
        self.depth_random_table.next_rng(rng) as i32
    }

    pub fn get_thinking_duration<T: Rng>(&self, rng: &mut T) -> Duration {
        Duration::from_secs_f32(rng.gen_range(self.thinking_time[0]..self.thinking_time[1]))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GamePhase {
    Opening,
    MiddleGame,
    EndGame,
}

impl GamePhase {
    pub fn new(board: &Board) -> Self {
        let minor_pieces = (board.pieces(Piece::Bishop) | board.pieces(Piece::Knight)).popcnt();
        let major_pieces = (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)).popcnt();
        let pawns = board.pieces(Piece::Pawn).popcnt();

        if pawns > 14 && minor_pieces == 4 && major_pieces >= 4 {
            GamePhase::Opening
        } else if pawns <= 14 && minor_pieces <= 4 && major_pieces <= 4 {
            GamePhase::EndGame
        } else {
            GamePhase::MiddleGame
        }
    }
}

fn eval_material(board: &Board, piece_weights: &[f32]) -> f32 {
    let mut material_score = 0.;
    for color in &[chess::Color::White, chess::Color::Black] {
        for piece in chess::ALL_PIECES {
            let piece_eval: f32;

            let piece_bb: chess::BitBoard = board.pieces(piece) & board.color_combined(*color);
            if matches!(piece, Piece::Pawn) {
                let doubled_pawns_count = doubled_pawns(&piece_bb, *color).popcnt();
                let isolated_pawns_count = isolated_pawns(&piece_bb).popcnt();
                let normal_pawn_count = piece_bb
                    .popcnt()
                    .checked_sub(doubled_pawns_count + isolated_pawns_count)
                    .unwrap_or_default();

                piece_eval = normal_pawn_count as f32 * 1.0
                    + doubled_pawns_count as f32 * 0.5
                    + isolated_pawns_count as f32 * 0.5;
            } else {
                piece_eval = piece_weights[piece.to_index()] * piece_bb.popcnt() as f32;
            }

            material_score += piece_eval
                * match color {
                    chess::Color::White => 1.,
                    chess::Color::Black => -1.,
                };
        }
    }

    material_score
}

lazy_static! {
    pub static ref BEST_PIECE_SQUARE_PHASES: PieceSquarePhases = {
        let pawns = vec![
            0., 0., 0., 0., 0., 0., 0., 0., 5., 5., 5., 5., 5., 5., 5., 5., 1., 1., 2., 3., 3., 2.,
            1., 1., 0.5, 0.5, 1., 2.5, 2.5, 1., 0.5, 0.5, 0., 0., 0., 2., 2., 0., 0., 0., 0.5,
            -0.5, -1., 0., 0., -1., -0.5, 0.5, 0.5, 1., 1., -2., -2., 1., 1., 0.5, 0., 0., 0., 0.,
            0., 0., 0., 0.,
        ];

        let knights = vec![
            -5., -4., -3., -3., -3., -3., -4., -5., -4., -2., 0., 0., 0., 0., -2., -4., -3., 0.,
            1., 1.5, 1.5, 1., 0., -3., -3., 0.5, 1.5, 2., 2., 1.5, 0.5, -3., -3., 0., 1.5, 2., 2.,
            1.5, 0., -3., -3., 0.5, 1., 1.5, 1.5, 1., 0.5, -3., -4., -2., 0., 0.5, 0.5, 0., -2.,
            -4., -5., -4., -3., -3., -3., -3., -4., -5.,
        ];

        let bishops = vec![
            -2., -1., -1., -1., -1., -1., -1., -2., -1., 0., 0., 0., 0., 0., 0., -1., -1., 0., 0.5,
            1., 1., 0.5, 0., -1., -1., 0.5, 0.5, 1., 1., 0.5, 0.5, -1., -1., 0., 1., 1., 1., 1.,
            0., -1., -1., 1., 1., 1., 1., 1., 1., -1., -1., 0.5, 0., 0., 0., 0., 0.5, -1., -2.,
            -1., -1., -1., -1., -1., -1., -2.,
        ];

        let rooks = vec![
            0., 0., 0., 0.5, 0.5, 0., 0., 0., -0.5, -1., -1., -1., -1., -1., -1., -0.5, -0.5, -1.,
            -1., -1., -1., -1., -1., -0.5, -0.5, -1., -1., -1., -1., -1., -1., -0.5, -0.5, -1.,
            -1., -1., -1., -1., -1., -0.5, -0.5, -1., -1., -1., -1., -1., -1., -0.5, 0.5, 1., 1.,
            1., 1., 1., 1., 0.5, 0., 0., 0., 0., 0., 0., 0., 0.,
        ];

        let queens = vec![
            -2., -1., -1., -0.5, -0.5, -1., -1., -2., -1., 0., 0., 0., 0., 0., 0., -1., -1., 0.,
            0.5, 0.5, 0.5, 0.5, 0., -1., -0.5, 0., 0.5, 0.5, 0.5, 0.5, 0., -0.5, 0., 0., 0.5, 0.5,
            0.5, 0.5, 0., -0.5, -1., 0.5, 0.5, 0.5, 0.5, 0.5, 0., -1., -1., 0., 0., 0., 0., 0., 0.,
            -1., -2., -1., -1., -0.5, -0.5, -1., -1., -2.,
        ];

        let king_early_game = vec![
            -3., -4., -4., -5., -5., -4., -4., -3., -3., -4., -4., -5., -5., -4., -4., -3., -3.,
            -4., -4., -5., -5., -4., -4., -3., -3., -4., -4., -5., -5., -4., -4., -3., -2., -3.,
            -3., -4., -4., -3., -3., -2., -1., -2., -2., -2., -2., -2., -2., -1., 2., 2., 0., 0.,
            0., 0., 2., 2., 2., 3., 1., 0., 0., 1., 3., 2.,
        ];

        let king_end_game = vec![
            0., 0., 0., 0.5, 0.5, 0., 0., 0., -0.5, -1., -1., -1., -1., -1., -1., -0.5, -0.5, -1.,
            -1., -1., -1., -1., -1., -0.5, -0.5, -1., -1., -1., -1., -1., -1., -0.5, -0.5, -1.,
            -1., -1., -1., -1., -1., -0.5, -0.5, -1., -1., -1., -1., -1., -1., -0.5, 0.5, 1., 1.,
            1., 1., 1., 1., 0.5, 0., 0., 0., 0., 0., 0., 0., 0.,
        ];

        PieceSquarePhases {
            opening: PieceSquareTables {
                pawn: pawns.clone(),
                knight: knights.clone(),
                bishop: bishops.clone(),
                rook: rooks.clone(),
                queen: queens.clone(),
                king: king_early_game.clone(),
            },
            middle_game: PieceSquareTables {
                pawn: pawns.clone(),
                knight: knights.clone(),
                bishop: bishops.clone(),
                rook: rooks.clone(),
                queen: queens.clone(),
                king: king_early_game.clone(),
            },
            end_game: PieceSquareTables {
                pawn: pawns.clone(),
                knight: knights.clone(),
                bishop: bishops.clone(),
                rook: rooks.clone(),
                queen: queens.clone(),
                king: king_end_game.clone(),
            },
        }
    };
}

pub fn eval(evaluation_presets: &EvaluationPresets, board: &Board) -> f32 {
    let material_score = eval_material(board, &evaluation_presets.piece_weights);

    let position_score = {
        let phase: GamePhase = GamePhase::new(board);
        evaluate_piece_square_location(&evaluation_presets.piece_square_phases, board, phase)
    };

    let mobility_score = {
        let colors = if board.side_to_move() == chess::Color::White {
            &[chess::Color::White, chess::Color::Black]
        } else {
            &[chess::Color::Black, chess::Color::White]
        };
        let mut mobility_score = 0.0;
        let mut board = board.clone();
        for color in colors {
            let mobility = MoveGen::new_legal(&board).count() as f32;
            mobility_score += match color {
                chess::Color::White => mobility,
                chess::Color::Black => -mobility,
            };
            board = match board.null_move() {
                Some(b) => b,
                None => break,
            }
        }
        mobility_score
    };

    // Add check bonus
    let checkers_score = {
        let mut checkers_score = 0.0;
        let checkers_bb = board.checkers();
        let white_checkers = checkers_bb & board.color_combined(chess::Color::White);
        checkers_score += white_checkers.popcnt() as f32 * evaluation_presets.check_bonus;

        let black_checkers = checkers_bb & board.color_combined(chess::Color::Black);
        checkers_score -= black_checkers.popcnt() as f32 * evaluation_presets.check_bonus;

        checkers_score
    };

    return material_score + mobility_score + checkers_score + position_score;
}

fn quiesce(
    evaluation_presets: &EvaluationPresets,
    board: &chess::Board,
    mut alpha: f32,
    beta: f32,
) -> f32 {
    let status = board.status();
    match status {
        chess::BoardStatus::Checkmate => {
            return f32::MIN;
        }
        chess::BoardStatus::Stalemate => return 0.,
        _ => (),
    }

    let who_to_move_mul = match board.side_to_move() {
        chess::Color::White => 1.,
        chess::Color::Black => -1.,
    };

    let stand_pat = eval(evaluation_presets, board) * who_to_move_mul;
    if stand_pat >= beta {
        return beta;
    }

    if alpha < stand_pat {
        alpha = stand_pat;
    }

    let moves = MoveGen::new_legal(&board)
        .filter(|chess_move| board.piece_on(chess_move.get_dest()) != None);
    for chess_move in moves {
        let updated_board = board.make_move_new(chess_move);
        let score = -quiesce(evaluation_presets, &updated_board, -beta, -alpha);

        if score >= beta {
            return beta;
        }

        if score > alpha {
            alpha = score;
        }
    }

    alpha
}

fn nega_max_alpha_beta_internal(
    #[cfg(not(target_arch = "wasm32"))] mut transpose_table: Arc<Mutex<TranspositionTable>>,
    #[cfg(target_arch = "wasm32")] transpose_table: &mut TranspositionTable,
    evaluation_presets: &EvaluationPresets,
    board: &chess::Board,
    depth: i32,
    half_move_count: u16,
    mut alpha: f32,
    beta: f32,
    #[cfg(not(target_arch = "wasm32"))] should_stop: Arc<Mutex<bool>>,
) -> f32 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if *should_stop.lock().unwrap() {
            return 0.;
        }
    }

    if let Some(stored_score) = transpose_table.get(board, depth) {
        match stored_score.flag {
            SearchFlag::Exact => return stored_score.score,
            SearchFlag::UpperBound if stored_score.score <= alpha => return stored_score.score,
            SearchFlag::Lowerbound if stored_score.score >= beta => return stored_score.score,
            _ => (),
        }
    }

    if depth == 0 {
        return quiesce(evaluation_presets, &board, alpha, beta);
    }

    let mut best_move = None;
    let mut is_exact = true;
    let moves = transpose_table.legal_moves(board);

    for chess_move in moves {
        let updated_board = board.make_move_new(chess_move);

        let score = -nega_max_alpha_beta_internal(
            #[cfg(not(target_arch = "wasm32"))]
            transpose_table.clone(),
            #[cfg(target_arch = "wasm32")]
            transpose_table,
            evaluation_presets,
            &updated_board,
            depth - 1,
            half_move_count + 1,
            -beta,
            -alpha,
            #[cfg(not(target_arch = "wasm32"))]
            should_stop.clone(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        if *should_stop.lock().unwrap() {
            return 0.;
        }

        if score > beta {
            transpose_table.add(
                board,
                SearchResult::new(
                    depth,
                    score,
                    SearchFlag::UpperBound,
                    half_move_count,
                    Some(chess_move),
                ),
            );
            return beta;
        }

        if score > alpha {
            alpha = score;
            best_move = Some(chess_move);
        } else {
            is_exact = false;
        }
    }

    transpose_table.add(
        board,
        SearchResult::new(
            depth,
            alpha,
            if is_exact {
                SearchFlag::Exact
            } else {
                SearchFlag::Lowerbound
            },
            half_move_count,
            best_move,
        ),
    );
    alpha
}

pub fn nega_max_alpha_beta(
    #[cfg(not(target_arch = "wasm32"))] transpose_table: Arc<Mutex<TranspositionTable>>,
    #[cfg(target_arch = "wasm32")] transpose_table: &mut TranspositionTable,
    evaluation_presets: &EvaluationPresets,
    board: &chess::Board,
    depth: i32,
    half_move_count: u16,
    #[cfg(not(target_arch = "wasm32"))] should_stop: Arc<Mutex<bool>>,
) -> i64 {
    let score = nega_max_alpha_beta_internal(
        transpose_table,
        evaluation_presets,
        board,
        depth,
        half_move_count,
        f32::MIN,
        f32::MAX,
        #[cfg(not(target_arch = "wasm32"))]
        should_stop,
    );

    (score * 10000.).round() as i64
}

fn evaluate_piece_square_location(
    piece_square_phases: &PieceSquarePhases,
    board: &Board,
    phase: GamePhase,
) -> f32 {
    let mut position_score = 0.;

    for color in chess::ALL_COLORS {
        for piece in chess::ALL_PIECES {
            let bb = board.pieces(piece) & board.color_combined(color);
            let square_table = piece_square_phases.get_square_table(phase, piece);

            for square in bb {
                let mut index = square.to_index();
                if color == chess::Color::Black {
                    index = 63 - index;
                }

                let square_value = square_table[index] / 24.0;

                position_score += match color {
                    chess::Color::White => square_value,
                    chess::Color::Black => -square_value,
                };
            }
        }
    }

    position_score
}

const DEFAULT_PIECE_WEIGHTS: [f32; 6] = [1.0, 3.0, 3.0, 5.0, 9.0, 0.0];

pub fn blunder_score(board: &Board, depth: i32) -> f32 {
    let who_to_move_mul = match board.side_to_move() {
        chess::Color::White => 1.,
        chess::Color::Black => -1.,
    };

    if depth == 0 {
        return eval_material(board, &DEFAULT_PIECE_WEIGHTS) * who_to_move_mul;
    }

    let mut max = f32::MIN;

    let moves = MoveGen::new_legal(board);
    for chess_move in moves {
        let updated_board = board.make_move_new(chess_move);
        let score = -blunder_score(&updated_board, depth - 1);

        if score > max {
            max = score;
        }
    }

    max
}

fn north_fill(bb: &BitBoard) -> BitBoard {
    let mut gen = bb.0;
    gen |= gen << 8;
    gen |= gen << 16;
    gen |= gen << 32;
    return BitBoard(gen);
}

fn south_fill(bb: &BitBoard) -> BitBoard {
    let mut gen = bb.0;
    gen |= gen >> 8;
    gen |= gen >> 16;
    gen |= gen >> 32;
    return BitBoard(gen);
}

fn file_fill(bb: &BitBoard) -> BitBoard {
    return north_fill(bb) | south_fill(bb);
}

fn north_one(bb: &BitBoard) -> BitBoard {
    return BitBoard(bb.0 << 8);
}

fn south_one(bb: &BitBoard) -> BitBoard {
    return BitBoard(bb.0 >> 8);
}

fn front_spans(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    match color {
        chess::Color::White => north_one(&north_fill(pawns)),
        chess::Color::Black => south_one(&south_fill(pawns)),
    }
}

fn rear_spans(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    match color {
        chess::Color::White => south_one(&south_fill(pawns)),
        chess::Color::Black => north_one(&north_fill(pawns)),
    }
}

fn pawns_behind_own(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    return pawns & rear_spans(&pawns, color);
}

fn pawns_in_front_own(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    return pawns & front_spans(&pawns, color);
}

fn pawns_in_front_and_behind_own(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    return pawns_in_front_own(pawns, color) & pawns_behind_own(pawns, color);
}

fn doubled_pawns(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    return pawns_in_front_own(pawns, color);
}

#[allow(dead_code)]
fn tripled_pawns(pawns: &BitBoard, color: chess::Color) -> BitBoard {
    return pawns_in_front_and_behind_own(pawns, color);
}

const NOT_A_FILE: BitBoard = BitBoard(0xfefefefefefefefe); // ~0x0101010101010101
const NOT_H_FILE: BitBoard = BitBoard(0x7f7f7f7f7f7f7f7f); // ~0x8080808080808080

fn east_one(bb: &BitBoard) -> BitBoard {
    return BitBoard(bb.0 << 1) & NOT_A_FILE;
}

fn west_one(bb: &BitBoard) -> BitBoard {
    return BitBoard(bb.0 >> 1) & NOT_H_FILE;
}

fn east_attack_file_fill(pawns: &BitBoard) -> BitBoard {
    east_one(&file_fill(pawns))
}

fn west_attack_file_fill(pawns: &BitBoard) -> BitBoard {
    west_one(&file_fill(pawns))
}

fn no_neighbor_on_east_file(pawns: &BitBoard) -> BitBoard {
    pawns & !west_attack_file_fill(pawns)
}

fn no_neighbor_on_west_file(pawns: &BitBoard) -> BitBoard {
    pawns & !east_attack_file_fill(pawns)
}

fn isolated_pawns(pawns: &BitBoard) -> BitBoard {
    no_neighbor_on_east_file(pawns) & no_neighbor_on_west_file(pawns)
}
