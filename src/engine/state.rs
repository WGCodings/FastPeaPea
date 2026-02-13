use shakmaty::{Chess, EnPassantMode, Position};
use shakmaty::zobrist::{Zobrist64};

use crate::engine::tt::TranspositionTable;

pub struct EngineState {
    pub position: Chess,
    pub repetition_stack: Vec<u64>,
    pub tt: TranspositionTable,
}

impl EngineState {
    pub fn new(tt_size :usize ) -> Self {
        let position = Chess::new();
        let mut repetition_stack: Vec<u64> = Vec::with_capacity(256);

        let hash =
            position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

        repetition_stack.push(hash);

        Self {
            position,
            repetition_stack,
            tt: TranspositionTable::new(tt_size),
        }
    }

    pub fn init_history(&mut self) {
        self.repetition_stack.clear();

        let hash =
            self.position
                .zobrist_hash::<Zobrist64>(EnPassantMode::Legal)
                .0;

        self.repetition_stack.push(hash);
    }

    pub fn increase_history(&mut self) {
        let hash =
            self.position
                .zobrist_hash::<Zobrist64>(EnPassantMode::Legal)
                .0;

        self.repetition_stack.push(hash);
    }
}
