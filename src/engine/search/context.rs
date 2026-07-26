use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::{Duration, Instant};
use shakmaty::{Chess, Color, Move, Position, Role, Square};
use crate::engine::corrhist::{CorrectionHistoryTable, MajorsAndKingsKey, MaterialKey, MinorsAndKingsKey, PawnKey};
use crate::engine::history::HistoryTables;
use crate::engine::params::Params;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::search::search::SearchStats;
use crate::engine::tt::TranspositionTable;
use crate::nnue::network::{accumulators_from_position, calculate_index, role_index, Accumulator, Network};

// Keep track of move, eval and nr of double ext per ply.
pub struct Stack {
    pub moves: [Option<Move>; 128],
    pub evals: [i32; 128],
    pub double_exts: [i32; 128],
}
// The searchcontext is passed on during the search and contains parameters, time management, history, tt tables etc
pub struct SearchContext<'a> {
    pub start_time: Instant,
    pub time_limit: Duration,
    pub node_limit : u64,

    pub stop: Arc<AtomicBool>, // Arc to share across threads
    pub node_count: Arc<AtomicU64>,  // node counting over multiple threads
    pub is_main : bool, // Flag to check if this is a main or helper thread

    pub params: &'a Params, // Params struct loaded from yaml or default
    pub ordering: &'a MoveOrdering, // Used for ordering of moves

    pub stats: SearchStats, // Some search statistics

    // TODO make fixed length array from it
    pub repetition_stack: Vec<u64>, // Stack of moves from previous moves played in the game, important for 3 fold repetition
    pub tt: &'a TranspositionTable, // TT

    pub nnue: NNUEState, // State of NNUE i e accumulators
    pub network: &'a Network, // NNUE network

    pub killers: [[Option<Move>; 3]; 128],
    pub history: HistoryTables,


    // All corrhist tables
    pub corrhist_pawn : CorrectionHistoryTable<PawnKey>,
    pub corrhist_material: CorrectionHistoryTable<MaterialKey>,
    pub corrhist_minor: CorrectionHistoryTable<MinorsAndKingsKey>,
    pub corrhist_major: CorrectionHistoryTable<MajorsAndKingsKey>,
    
    pub stack : Stack,

    pub excluded_move: [Option<Move>; 128], // excluded moves for Singular extensions

}

impl<'a> SearchContext<'a> {

    // =====================================================================================================================//
    // THREEFOLD AND 50 MOVES                                                                                               //
    // =====================================================================================================================//
    #[inline(always)]
    pub fn is_threefold(&mut self, pos: &Chess) -> bool {

        let mut count = 0;

        let current = self.repetition_stack.last().unwrap_or(&0);
        let len = self.repetition_stack.len();

        if len == 0{
            return false;
        }

        // Avoid underflow
        let start = len.saturating_sub(pos.halfmoves() as usize + 1);

        // Scan backwards skipping last position
        for &hash in self.repetition_stack[start..len-1].iter().rev() {

            if hash == *current {
                count += 1;
                if count >= 1 {
                    return true; // 1-fold repetition
                }
            }
        }

        false
    }
    #[inline(always)]
    pub fn is_50_moves(&self,pos: &Chess) -> bool {
        pos.halfmoves()>= 100
    }

    // =====================================================================================================================//
    // REPETITION MANAGEMENT                                                                                                //
    // =====================================================================================================================//
    #[inline(always)]
    pub fn increase_history(&mut self, hash : u64) {
        self.repetition_stack.push(hash);
    }
    #[inline(always)]
    pub fn decrease_history(&mut self) {
        self.repetition_stack.pop();
    }

    // =====================================================================================================================//
    // KILLER MOVES                                                                                                         //
    // =====================================================================================================================//
    #[inline(always)]
    pub fn store_killer(&mut self, ply: usize, mv: Move) {
        // Do not store duplicates
        if self.killers[ply][0] == Some(mv) {
            return;
        }

        // Shift old killer
        self.killers[ply][2] = self.killers[ply][1];
        self.killers[ply][1] = self.killers[ply][0];
        self.killers[ply][0] = Some(mv);
    }
    #[inline(always)]
    pub fn clear_killers_at(&mut self,ply:usize) {
        self.killers[ply][0] = None;
        self.killers[ply][1] = None;
        self.killers[ply][2] = None;
    }



    // =====================================================================================================================//
    // CHECK IF IMPROVING                                                                                                   //
    // =====================================================================================================================//

    #[inline(always)]
    pub fn is_improving(&self, ply: usize) -> bool {
        if ply < 2 {
            return false;
        }

        self.stack.evals[ply] > self.stack.evals[ply - 2]
    }
}

// =====================================================================================================================//
// KEEP TRACK OF CHANGES TO ACCUMULATOR DURING MAKE MOVE                                                                //
// =====================================================================================================================//
#[derive(Clone, Copy)]
pub struct AccumulatorDelta {
    removed: [(usize, usize); 2],
    added:   [(usize, usize); 2],
    n_removed: u8,
    n_added: u8,
    is_clean : bool
}

impl AccumulatorDelta {
    fn default() -> Self {
        Self {
            removed: [(0, 0); 2],
            added: [(0, 0); 2],
            n_removed: 0,
            n_added: 0,
            is_clean: false,
        }
    }
}

// =====================================================================================================================//
// STATE OF ACCUMULATOR                                                                                                 //
// =====================================================================================================================//

pub struct NNUEState {
    pub us: Accumulator,
    pub them: Accumulator,
    pub stack: Vec<AccumulatorDelta>,
}

impl NNUEState {
    pub fn new<P: Position>(pos: &P, net: &Network) -> Self {
        let (us, them) = accumulators_from_position(pos, net);
        Self {
            us,
            them,
            stack: Vec::with_capacity(64),
        }
    }
}

// =====================================================================================================================//
// MAKE AND UNMAKE NNUE ACCUMULATOR                                                                                     //
// =====================================================================================================================//

#[inline(always)]
pub fn unmake_move_nnue(net: &Network, state: &mut NNUEState) {
    let delta = state.stack.pop().unwrap();

    // No need to do anything if the accumulator delta is not clean
    if delta.is_clean {
        std::mem::swap(&mut state.us, &mut state.them);

        let mut us_add = [0usize; 2];
        let mut us_rem = [0usize; 2];
        let mut them_add = [0usize; 2];
        let mut them_rem = [0usize; 2];

        for k in 0..delta.n_added as usize {
            us_rem[k] = delta.added[k].0;
            them_rem[k] = delta.added[k].1;
        }
        for k in 0..delta.n_removed as usize {
            us_add[k] = delta.removed[k].0;
            them_add[k] = delta.removed[k].1;
        }

        state.us.apply_feature_updates(&us_add[..delta.n_removed as usize], &us_rem[..delta.n_added as usize], net);
        state.them.apply_feature_updates(&them_add[..delta.n_removed as usize], &them_rem[..delta.n_added as usize], net);
    }

}

#[inline(always)]
pub fn make_null_move_nnue(state: &mut NNUEState) {
    let delta = AccumulatorDelta::default();

    state.stack.push(delta);
    //std::mem::swap(&mut state.us, &mut state.them);
}

#[inline(always)]
pub fn unmake_null_move_nnue(state: &mut NNUEState) {
    let delta = state.stack.pop().unwrap();

    if delta.is_clean {
        std::mem::swap(&mut state.us, &mut state.them);
    }

}

#[inline(always)]
/// Loop over the accumulator delta stack until you find a clean one. Then do incremental updates from that point back to the current accumulator.
pub fn clean_accumulator(
    net: &Network,
    state: &mut NNUEState){

    let mut start = state.stack.len();
    while start > 0 && !state.stack[start - 1].is_clean {
        start -= 1;
    }

    // loop over acc stack to apply all dirty updates
    for i in start..state.stack.len() {


        let delta = state.stack[i];

        let mut us_add = [0usize; 2];
        let mut us_rem = [0usize; 2];
        let mut them_add = [0usize; 2];
        let mut them_rem = [0usize; 2];

        for k in 0..delta.n_added as usize {
            us_add[k] = delta.added[k].0;
            them_add[k] = delta.added[k].1;
        }
        for k in 0..delta.n_removed as usize {
            us_rem[k] = delta.removed[k].0;
            them_rem[k] = delta.removed[k].1;
        }

        state.us.apply_feature_updates(&us_add[..delta.n_added as usize], &us_rem[..delta.n_removed as usize], net);
        state.them.apply_feature_updates(&them_add[..delta.n_added as usize], &them_rem[..delta.n_removed as usize], net);

        // if applied, it is clean
        state.stack[i].is_clean = true;
        std::mem::swap(&mut state.us, &mut state.them);
        }
    }





#[inline(always)]
pub fn make_move_nnue<P: Position>(
    pos: &P,
    mv: &Move,
    state: &mut NNUEState,
) {
    let mut delta = AccumulatorDelta::default();

    // record all changes to accumulator inside these lists to apply them fused later
    let mut us_add = [0usize; 2];
    let mut us_rem = [0usize; 2];
    let mut them_add = [0usize; 2];
    let mut them_rem = [0usize; 2];

    let perspective = pos.turn();
    let board = pos.board();

    match *mv {

        // ============================================================
        // NORMAL MOVE (may include capture + promotion)
        // ============================================================
        Move::Normal { from, to, promotion, .. } => {
            let piece = board.piece_at(from).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };
            let from_sq = from.to_usize();
            let to_sq = to.to_usize();
            let piece_type = role_index(piece.role);

            remove_feature(&mut delta, &mut us_rem, &mut them_rem, side, from_sq, piece_type, perspective);

            if let Some(captured) = board.piece_at(to) {
                let cap_side = if captured.color == Color::White { 0 } else { 1 };
                let cap_type = role_index(captured.role);

                remove_feature(&mut delta, &mut us_rem, &mut them_rem, cap_side, to_sq, cap_type, perspective);
            }

            let final_role = promotion.unwrap_or(piece.role);
            let final_type = role_index(final_role);

            add_feature(&mut delta, &mut us_add, &mut them_add, side, to_sq, final_type, perspective);
        }

        // ============================================================
        // EN PASSANT
        // ============================================================
        Move::EnPassant { from, to } => {
            let piece = board.piece_at(from).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };

            let from_sq = from.to_usize();
            let to_sq = to.to_usize();
            let piece_type = role_index(Role::Pawn);

            remove_feature(&mut delta, &mut us_rem, &mut them_rem, side, from_sq, piece_type, perspective);

            let captured_sq = Square::from_coords(to.file(), from.rank());
            let cap_sq = captured_sq.to_usize();
            let cap_side = 1 - side;

            remove_feature(&mut delta, &mut us_rem, &mut them_rem, cap_side, cap_sq, piece_type, perspective);

            add_feature(&mut delta, &mut us_add, &mut them_add, side, to_sq, piece_type, perspective);
        }

        // ============================================================
        // CASTLING
        // ============================================================
        Move::Castle { king, rook } => {
            let piece = board.piece_at(king).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };

            let king_from = king.to_usize();
            let rook_from = rook.to_usize();
            let kingside = rook.file() > king.file();

            let king_to = if kingside { king_from + 2 } else { king_from - 2 };
            let rook_to = if kingside { king_from + 1 } else { king_from - 1 };

            let king_type = role_index(Role::King);
            let rook_type = role_index(Role::Rook);

            remove_feature(&mut delta, &mut us_rem, &mut them_rem, side, king_from, king_type, perspective);
            remove_feature(&mut delta, &mut us_rem, &mut them_rem, side, rook_from, rook_type, perspective);
            add_feature(&mut delta, &mut us_add, &mut them_add, side, king_to, king_type, perspective);
            add_feature(&mut delta, &mut us_add, &mut them_add, side, rook_to, rook_type, perspective);
        }
        _ => {}
    }

    // Apply all fused updates to accumulator at once
    //state.us.apply_feature_updates(&us_add[..delta.n_added as usize], &us_rem[..delta.n_removed as usize], net);
    //state.them.apply_feature_updates(&them_add[..delta.n_added as usize], &them_rem[..delta.n_removed as usize], net);

    state.stack.push(delta);
    //std::mem::swap(&mut state.us, &mut state.them);
}



// =====================================================================================================================//
// HELPER FUNCTION TO ACTIVATE AND DEACTIVATE FEATURES INTO FUSED UPDATES                                               //
// =====================================================================================================================//

fn remove_feature(
    delta: &mut AccumulatorDelta,
    us_rem: &mut [usize; 2],
    them_rem: &mut [usize; 2],
    side: usize,
    sq: usize,
    piece_type: usize,
    perspective: Color,
) {
    let us_idx = calculate_index(side, sq, piece_type, perspective);
    let them_idx = calculate_index(side, sq, piece_type, !perspective);

    let n = delta.n_removed as usize;
    us_rem[n] = us_idx;
    them_rem[n] = them_idx;
    delta.removed[n] = (us_idx, them_idx);
    delta.n_removed += 1;
}

fn add_feature(
    delta: &mut AccumulatorDelta,
    us_add: &mut [usize; 2],
    them_add: &mut [usize; 2],
    side: usize,
    sq: usize,
    piece_type: usize,
    perspective: Color,
) {
    let us_idx = calculate_index(side, sq, piece_type, perspective);
    let them_idx = calculate_index(side, sq, piece_type, !perspective);

    let n = delta.n_added as usize;
    us_add[n] = us_idx;
    them_add[n] = them_idx;
    delta.added[n] = (us_idx, them_idx);
    delta.n_added += 1;
}


