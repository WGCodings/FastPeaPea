use shakmaty::{Chess, EnPassantMode};
use shakmaty::zobrist::{Zobrist64, ZobristHash};

pub struct EngineState {
    pub position: Chess,
    pub repetition_stack: Vec<u64>,
}

impl EngineState {
    pub fn new() -> Self {
        let position = Chess::default();
        let mut repetition_stack : Vec<u64> = Vec::with_capacity(256);
        let hash = position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;
        repetition_stack.push(hash);

        Self {
            position,
            repetition_stack,
        }
    }

    pub fn init_history(&mut self) {
        self.repetition_stack.clear();
        let hash = self.position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;
        self.repetition_stack.push(hash);
    }

    pub fn increase_history(&mut self) {
        let hash = self.position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;
        self.repetition_stack.push(hash);
    }
}
