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
use crate::nnue::network::{accumulator_for_perspective, calculate_index, role_index, Accumulator, BucketInfo, Network};

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
    white_removed: [usize; 2],
    white_added: [usize; 2],
    black_removed: [usize; 2],
    black_added: [usize; 2],
    n_removed: u8,
    n_added: u8,
    is_refresh: bool,
    refresh_color: Color
}

impl AccumulatorDelta {
    fn default() -> Self {
        Self {
            white_removed: [0; 2], white_added: [0; 2],
            black_removed: [0; 2], black_added: [0; 2],
            n_removed: 0, n_added: 0,
            is_refresh: false,
            refresh_color: Color::White,
        }
    }
}

// =====================================================================================================================//
// STATE OF ACCUMULATOR                                                                                                 //
// =====================================================================================================================//

pub struct NNUEState {
    pub white_acc: Accumulator,
    pub black_acc: Accumulator,
    pub white_info: BucketInfo,
    pub black_info: BucketInfo,
    pub stack: Vec<AccumulatorDelta>,
    pub applied: usize
}

impl NNUEState {
    pub fn new<P: Position>(pos: &P, net: &Network) -> Self {
        let (white_acc, white_info) = accumulator_for_perspective(pos, net, Color::White);
        let (black_acc, black_info) = accumulator_for_perspective(pos, net, Color::Black);
        Self { white_acc, black_acc, white_info, black_info, stack: Vec::with_capacity(64), applied: 0 }
    }
}

// =====================================================================================================================//
// MAKE AND UNMAKE NNUE ACCUMULATOR                                                                                     //
// =====================================================================================================================//

#[inline(always)]
/// Loop over the accumulator delta stack until you find a clean one. Then do incremental updates from that point back to the current accumulator.
pub fn clean_accumulator(
    net: &Network,
    state: &mut NNUEState){

    if state.applied == state.stack.len() {
        return;
    }
    for i in state.applied..state.stack.len() {
        let delta = state.stack[i];

        if delta.is_refresh { continue; }

        state.white_acc.apply_feature_updates(&delta.white_added[..delta.n_added as usize], &delta.white_removed[..delta.n_removed as usize], net);
        state.black_acc.apply_feature_updates(&delta.black_added[..delta.n_added as usize], &delta.black_removed[..delta.n_removed as usize], net);
    }
    state.applied = state.stack.len();
    }

/// Check if a move is a king move
/// TODO Later add only check if it is moving to new bucket
#[inline(always)]
fn is_king_move<P: Position>(mv: &Move) -> bool {
    match *mv {
        Move::Normal { role, .. } => role == Role::King,
        Move::Castle { .. } => true,
        _ => false,
    }
}



#[inline(always)]
pub fn make_move_nnue<P: Position>(pos: &P, child_pos: &P, mv: &Move, net: &Network, state: &mut NNUEState) {
    let mut delta = AccumulatorDelta::default();
    let board = pos.board();
    let white_info = state.white_info;
    let black_info = state.black_info;

    match *mv {
        Move::Normal { from, to, promotion, .. } => {
            let piece = board.piece_at(from).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };
            let piece_type = role_index(piece.role);

            remove_feature(&mut delta, side, from.to_usize(), piece_type, white_info, black_info);

            if let Some(captured) = board.piece_at(to) {
                let cap_side = if captured.color == Color::White { 0 } else { 1 };
                let cap_type = role_index(captured.role);
                remove_feature(&mut delta, cap_side, to.to_usize(), cap_type, white_info, black_info);
            }

            let final_type = role_index(promotion.unwrap_or(piece.role));
            add_feature(&mut delta, side, to.to_usize(), final_type, white_info, black_info);
        }

        Move::EnPassant { from, to } => {
            let piece = board.piece_at(from).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };
            let piece_type = role_index(Role::Pawn);

            remove_feature(&mut delta, side, from.to_usize(), piece_type, white_info, black_info);

            let cap_sq = Square::from_coords(to.file(), from.rank()).to_usize();
            remove_feature(&mut delta, 1 - side, cap_sq, piece_type, white_info, black_info);

            add_feature(&mut delta, side, to.to_usize(), piece_type, white_info, black_info);
        }

        Move::Castle { king, rook } => {
            let piece = board.piece_at(king).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };

            let king_from = king.to_usize();
            let rook_from = rook.to_usize();
            let kingside = rook.file() > king.file();
            let king_to = if kingside { king_from + 2 } else { king_from - 2 };
            let rook_to = if kingside { king_from + 1 } else { king_from - 1 };

            remove_feature(&mut delta, side, king_from, role_index(Role::King), white_info, black_info);
            remove_feature(&mut delta, side, rook_from, role_index(Role::Rook), white_info, black_info);
            add_feature(&mut delta, side, king_to, role_index(Role::King), white_info, black_info);
            add_feature(&mut delta, side, rook_to, role_index(Role::Rook), white_info, black_info);
        }
        _ => {}
    }

    if is_king_move::<P>(mv) {
        let stm = pos.turn();

        clean_accumulator(net, state);

        match stm {
            Color::White => state.black_acc.apply_feature_updates(
                &delta.black_added[..delta.n_added as usize],
                &delta.black_removed[..delta.n_removed as usize],
                net,
            ),
            Color::Black => state.white_acc.apply_feature_updates(
                &delta.white_added[..delta.n_added as usize],
                &delta.white_removed[..delta.n_removed as usize],
                net,
            ),
        }

        let (fresh_acc, fresh_info) = accumulator_for_perspective(child_pos, net, stm);
        match stm {
            Color::White => { state.white_acc = fresh_acc; state.white_info = fresh_info; }
            Color::Black => { state.black_acc = fresh_acc; state.black_info = fresh_info; }
        }

        delta.is_refresh = true;
        delta.refresh_color = stm;
        state.stack.push(delta);
        state.applied += 1;
        return;
    }

    state.stack.push(delta);
}

#[inline(always)]
pub fn unmake_move_nnue<P: Position>(pos: &P, net: &Network, state: &mut NNUEState) {
    let is_clean = state.applied == state.stack.len();
    let delta = state.stack.pop().unwrap();

    if !is_clean {
        return;
    }
    state.applied -= 1;

    if delta.is_refresh {
        let stm = delta.refresh_color;

        match stm {
            Color::White => state.black_acc.apply_feature_updates(
                &delta.black_removed[..delta.n_removed as usize],
                &delta.black_added[..delta.n_added as usize],
                net,
            ),
            Color::Black => state.white_acc.apply_feature_updates(
                &delta.white_removed[..delta.n_removed as usize],
                &delta.white_added[..delta.n_added as usize],
                net,
            ),
        }

        let (fresh_acc, fresh_info) = accumulator_for_perspective(pos, net, stm);
        match stm {
            Color::White => { state.white_acc = fresh_acc; state.white_info = fresh_info; }
            Color::Black => { state.black_acc = fresh_acc; state.black_info = fresh_info; }
        }
        return;
    }

    state.white_acc.apply_feature_updates(
        &delta.white_removed[..delta.n_removed as usize],
        &delta.white_added[..delta.n_added as usize],
        net,
    );
    state.black_acc.apply_feature_updates(
        &delta.black_removed[..delta.n_removed as usize],
        &delta.black_added[..delta.n_added as usize],
        net,
    );
}



// =====================================================================================================================//
// HELPER FUNCTION TO ACTIVATE AND DEACTIVATE FEATURES INTO FUSED UPDATES                                               //
// =====================================================================================================================//

fn remove_feature(delta: &mut AccumulatorDelta, side: usize, sq: usize, piece_type: usize, white_info: BucketInfo, black_info: BucketInfo) {
    let white_idx = calculate_index(side, sq, piece_type, Color::White, white_info);
    let black_idx = calculate_index(side, sq, piece_type, Color::Black, black_info);
    let n = delta.n_removed as usize;
    delta.white_removed[n] = white_idx;
    delta.black_removed[n] = black_idx;
    delta.n_removed += 1;
}

fn add_feature(delta: &mut AccumulatorDelta, side: usize, sq: usize, piece_type: usize, white_info: BucketInfo, black_info: BucketInfo) {
    let white_idx = calculate_index(side, sq, piece_type, Color::White, white_info);
    let black_idx = calculate_index(side, sq, piece_type, Color::Black, black_info);
    let n = delta.n_added as usize;
    delta.white_added[n] = white_idx;
    delta.black_added[n] = black_idx;
    delta.n_added += 1;
}


