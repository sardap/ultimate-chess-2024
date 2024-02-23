use bevy::prelude::*;
use chess::{Board, ChessMove, MoveGen};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::uchess::hash_board;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize_repr, Serialize_repr)]
#[repr(i32)]
pub enum SearchFlag {
    Exact = 0,
    Lowerbound = 1,
    UpperBound = 2,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SearchResult {
    depth: i32,
    pub score: f32,
    pub flag: SearchFlag,
    age: u16,
    best_move: Option<ChessMove>,
}

impl SearchResult {
    pub fn new(
        depth: i32,
        score: f32,
        flag: SearchFlag,
        age: u16,
        best_move: Option<ChessMove>,
    ) -> Self {
        Self {
            depth,
            score,
            flag,
            age,
            best_move,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct TranspositionTable {
    map: HashMap<u64, SearchResult>,
}

impl TranspositionTable {
    pub fn add(&mut self, board: &Board, search_result: SearchResult) {
        let hash = hash_board(board);
        let existing_depth = if let Some(existing) = self.map.get(&hash) {
            existing.depth
        } else {
            0
        };

        if existing_depth < search_result.depth
            && search_result.score != f32::MAX
            && search_result.score != f32::MIN
        {
            self.map.insert(hash_board(board), search_result);
        }
    }

    pub fn get(&self, board: &Board, depth: i32) -> Option<SearchResult> {
        match self.map.get(&hash_board(board)) {
            Some(result) => {
                if result.depth >= depth {
                    Some(*result)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn size(&self) -> usize {
        self.map.len()
    }

    pub fn legal_moves(&self, board: &Board) -> Vec<ChessMove> {
        let mut moves: Vec<_> = MoveGen::new_legal(&board).collect();

        if let Some(lookup) = self.map.get(&hash_board(board)) {
            if let Some(best_move) = lookup.best_move {
                if moves.contains(&best_move) {
                    moves.retain(|mov| *mov != best_move);
                    moves.insert(0, best_move);
                }
            }
        }

        moves
    }

    pub fn trim(&mut self, half_move_count: u16) {
        const HALF_MOVE_CUTOFF: i32 = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                10
            }
            #[cfg(target_arch = "wasm32")]
            {
                1
            }
        };

        if half_move_count <= HALF_MOVE_CUTOFF as u16 {
            return;
        }

        let mut to_remove = Vec::new();

        debug!(
            "Trimming transposition table current size {}, half move count {}",
            self.map.len(),
            half_move_count
        );

        for key in self.map.keys() {
            let entry = self.map.get(key).unwrap();

            if entry.age as i32 <= (half_move_count as i32 - HALF_MOVE_CUTOFF).abs() {
                to_remove.push(*key);
            }
        }

        for key in to_remove {
            self.map.remove(&key);
        }

        debug!("Trimming transposition table complete {}", self.map.len(),);
    }
}

#[allow(dead_code)]
pub trait TranspositionTableTrait {
    fn add(&mut self, board: &Board, search_result: SearchResult);
    fn get(&self, board: &Board, depth: i32) -> Option<SearchResult>;
    fn legal_moves(&self, key: &Board) -> Vec<ChessMove>;
    fn trim(&mut self, half_move_count: u16);
}

impl TranspositionTableTrait for Arc<Mutex<TranspositionTable>> {
    fn add(&mut self, board: &Board, search_result: SearchResult) {
        let mut table = self.lock().unwrap();
        table.add(board, search_result);
    }

    fn get(&self, board: &Board, depth: i32) -> Option<SearchResult> {
        let table = self.lock().unwrap();
        table.get(board, depth)
    }

    fn legal_moves(&self, key: &Board) -> Vec<ChessMove> {
        let table = self.lock().unwrap();
        table.legal_moves(key)
    }

    fn trim(&mut self, half_move_count: u16) {
        let mut table = self.lock().unwrap();
        table.trim(half_move_count);
    }
}

impl TranspositionTableTrait for &mut TranspositionTable {
    fn add(&mut self, board: &Board, search_result: SearchResult) {
        TranspositionTable::add(self, board, search_result);
    }

    fn get(&self, board: &Board, depth: i32) -> Option<SearchResult> {
        TranspositionTable::get(self, board, depth)
    }

    fn legal_moves(&self, key: &Board) -> Vec<ChessMove> {
        TranspositionTable::legal_moves(self, key)
    }

    fn trim(&mut self, half_move_count: u16) {
        TranspositionTable::trim(self, half_move_count);
    }
}
