use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::{Duration, Instant};
use shakmaty::{attacks, Bitboard, Board, Chess, Color, Move, Position, Role, Square};
use crate::engine::corrhist::{CorrectionHistoryTable, MajorsAndKingsKey, MaterialKey, MinorsAndKingsKey, PawnKey};
use crate::engine::finny::{FinnyEntry, FinnyTable};
use crate::engine::history::HistoryTables;
use crate::engine::params::Params;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::search::search::SearchStats;
use crate::engine::tt::TranspositionTable;
use crate::nnue::network::{accumulator_for_perspective, attacks_from_for, calculate_index, calculate_threat_index, get_bucket, init_threats, role_index, Accumulator, Network};

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

const THREAT_CHANGE_SIZE: usize = 32;
#[derive(Clone, Copy)]
pub struct AccumulatorDelta {
    white_removed: [u16; 2],
    white_added: [u16; 2],
    black_removed: [u16; 2],
    black_added: [u16; 2],
    n_removed: u8,
    n_added: u8,

    threat_white_removed: [u16;THREAT_CHANGE_SIZE],
    threat_white_added: [u16;THREAT_CHANGE_SIZE],
    threat_black_removed: [u16;THREAT_CHANGE_SIZE],
    threat_black_added: [u16;THREAT_CHANGE_SIZE],
    n_threat_removed: u8,
    n_threat_added: u8,

    threat_squares_affected: [u16; THREAT_CHANGE_SIZE],
    n_threat_squares_affected: u8,

    is_refresh: bool,
    refresh_color: Color
}

impl AccumulatorDelta {
    fn default() -> Self {
        Self {
            white_removed: [0; 2], white_added: [0; 2],
            black_removed: [0; 2], black_added: [0; 2],
            n_removed: 0, n_added: 0,
            threat_white_removed: [0; THREAT_CHANGE_SIZE], threat_white_added: [0; THREAT_CHANGE_SIZE],
            threat_black_removed: [0; THREAT_CHANGE_SIZE], threat_black_added: [0; THREAT_CHANGE_SIZE],
            n_threat_removed: 0, n_threat_added: 0,
            threat_squares_affected: [0; THREAT_CHANGE_SIZE],
            n_threat_squares_affected: 0,
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
    pub white_bucket: usize,
    pub black_bucket: usize,
    pub white_is_mirrored : bool,
    pub black_is_mirrored : bool,
    pub stack: Vec<AccumulatorDelta>,
    pub applied: usize,
    pub ft: FinnyTable,
    pub threats: [Bitboard; 64]
}

impl NNUEState {
    pub fn new<P: Position>(pos: &P, net: &Network) -> Self {
        let (mut white_acc, white_bucket, white_is_mirrored) = accumulator_for_perspective(pos, net, Color::White);
        let (mut black_acc, black_bucket, black_is_mirrored) = accumulator_for_perspective(pos, net, Color::Black);

        let threats = init_threats(pos.board());
        for sq in 0..64 {
            for target in threats[sq] {
                white_acc.add_feature(calculate_threat_index(sq, target.to_usize(), Color::White, white_is_mirrored), net);
                black_acc.add_feature(calculate_threat_index(sq, target.to_usize(), Color::Black, black_is_mirrored), net);
            }
        }

        let mut ft = FinnyTable::default(net);
        let bb = get_bb(pos.board());
        *ft.get_entry(Color::White, white_bucket, white_is_mirrored) = FinnyEntry { acc: white_acc, piece_bb: bb ,threats};
        *ft.get_entry(Color::Black, black_bucket, black_is_mirrored) = FinnyEntry { acc: black_acc, piece_bb: bb ,threats };


        Self { white_acc, black_acc, white_bucket, black_bucket, white_is_mirrored, black_is_mirrored, stack: Vec::with_capacity(64), applied: 0, ft, threats }
    }
}

// =====================================================================================================================//
// MAKE AND UNMAKE NNUE ACCUMULATOR                                                                                     //
// =====================================================================================================================//

/// Loop over the accumulator delta stack until you find a clean one. Then do incremental updates from that point back to the current accumulator.
#[inline(always)]
pub fn clean_accumulator<P: Position>(pos: &P, net: &Network, state: &mut NNUEState) {

    if state.applied == state.stack.len() {
        return;
    }

    // flags used to avoid cleaning the accumulator when we are using finny tables later to do the updates anyways.
    let mut do_white_finny_refresh = false;
    let mut do_black_finny_refresh = false;

    for i in state.applied..state.stack.len() {
        let delta = state.stack[i];

        if delta.is_refresh {
            match delta.refresh_color {
                Color::White => {
                    do_white_finny_refresh = true;
                    if !do_black_finny_refresh {
                        state.black_acc.apply_feature_updates(&delta.black_added[..delta.n_added as usize], &delta.black_removed[..delta.n_removed as usize], net);
                        state.black_acc.apply_feature_updates(&delta.threat_black_added[..delta.n_threat_added as usize], &delta.threat_black_removed[..delta.n_threat_removed as usize], net);
                    }
                }
                Color::Black => {
                    do_black_finny_refresh = true;
                    if !do_white_finny_refresh {
                        state.white_acc.apply_feature_updates(&delta.white_added[..delta.n_added as usize], &delta.white_removed[..delta.n_removed as usize], net);
                        state.white_acc.apply_feature_updates(&delta.threat_white_added[..delta.n_threat_added as usize], &delta.threat_white_removed[..delta.n_threat_removed as usize], net);
                    }
                }
            }
            continue;
        }

        if !do_white_finny_refresh {
            state.white_acc.apply_feature_updates(&delta.white_added[..delta.n_added as usize], &delta.white_removed[..delta.n_removed as usize], net);
            state.white_acc.apply_feature_updates(&delta.threat_white_added[..delta.n_threat_added as usize], &delta.threat_white_removed[..delta.n_threat_removed as usize], net);
        }
        if !do_black_finny_refresh {
            state.black_acc.apply_feature_updates(&delta.black_added[..delta.n_added as usize], &delta.black_removed[..delta.n_removed as usize], net);
            state.black_acc.apply_feature_updates(&delta.threat_black_added[..delta.n_threat_added as usize], &delta.threat_black_removed[..delta.n_threat_removed as usize], net);
        }
    }

    if do_white_finny_refresh {
        state.white_acc = finny_refresh(pos, net, Color::White, state.white_bucket, state.white_is_mirrored, &state.threats, &mut state.ft);

    }
    if do_black_finny_refresh {
        state.black_acc = finny_refresh(pos, net, Color::Black, state.black_bucket, state.black_is_mirrored, &state.threats, &mut state.ft);
    }

    state.applied = state.stack.len();
}

fn finny_refresh<P: Position>(pos: &P, net: &Network, perspective: Color, bucket: usize, is_mirrored : bool, current_threats: &[Bitboard; 64], ft: &mut FinnyTable) -> Accumulator {
    let board = pos.board();
    let new_bb = get_bb(board);
    let entry = ft.get_entry(perspective, bucket, is_mirrored);

    let mut acc = entry.acc;

    let mut adds = [0u16; THREAT_CHANGE_SIZE];
    let mut rems = [0u16; THREAT_CHANGE_SIZE];
    let mut n_add = 0;
    let mut n_rem = 0;

    // Diff the base768
    for side in 0..2 {
        for role_idx in 0..6 {
            let old = entry.piece_bb[side][role_idx];
            let new = new_bb[side][role_idx];

            for sq in old & !new {
                rems[n_rem] = calculate_index(side, sq.to_usize(), role_idx, perspective, bucket, is_mirrored);
                n_rem += 1;
            }
            for sq in new & !old {
                adds[n_add] = calculate_index(side, sq.to_usize(), role_idx, perspective, bucket, is_mirrored);
                n_add += 1;
            }
        }
    }
    acc.apply_feature_updates(&adds[..n_add], &rems[..n_rem], net);


    // Diff the threats
    let mut tadds = [0u16; THREAT_CHANGE_SIZE];
    let mut trems = [0u16; THREAT_CHANGE_SIZE];
    let mut tn_add = 0;
    let mut tn_rem = 0;
    for sq in 0..64 {
        let old = entry.threats[sq];
        let new = current_threats[sq];
        if old == new { continue; }
        for target in old & !new { trems[tn_rem] = calculate_threat_index(sq, target.to_usize(), perspective, is_mirrored); tn_rem += 1; }
        for target in new & !old { tadds[tn_add] = calculate_threat_index(sq, target.to_usize(), perspective, is_mirrored); tn_add += 1; }
    }

    acc.apply_feature_updates(&tadds[..tn_add], &trems[..tn_rem], net);

    entry.acc = acc;
    entry.piece_bb = new_bb;
    entry.threats = *current_threats;
    acc
}

/// Check if a move is a king move
#[inline(always)]
fn is_king_move<P: Position>(mv: &Move) -> bool {
    match *mv {
        Move::Normal { role, .. } => role == Role::King,
        Move::Castle { .. } => true,
        _ => false,
    }
}

#[inline(always)]
pub fn make_move_nnue<P: Position>(pos: &P, child_pos: &P, mv: &Move, state: &mut NNUEState) {
    let mut delta = AccumulatorDelta::default();
    let board = pos.board();
    let white_bucket = state.white_bucket;
    let white_is_mirrored = state.white_is_mirrored;
    let black_bucket = state.black_bucket;
    let black_is_mirrored = state.black_is_mirrored;

    let mut touched = [0usize; 4];
    let n_touched: usize;

    match *mv {
        Move::Normal { from, to, promotion, .. } => {
            let piece = board.piece_at(from).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };
            let piece_type = role_index(piece.role);

            remove_feature(&mut delta, side, from.to_usize(), piece_type, white_bucket, white_is_mirrored,black_is_mirrored,black_bucket);

            if let Some(captured) = board.piece_at(to) {
                let cap_side = if captured.color == Color::White { 0 } else { 1 };
                let cap_type = role_index(captured.role);
                remove_feature(&mut delta, cap_side, to.to_usize(), cap_type, white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);
            }

            let final_type = role_index(promotion.unwrap_or(piece.role));
            add_feature(&mut delta, side, to.to_usize(), final_type, white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);

            // Keep track of touches squares to update threats
            touched[0] = from.to_usize();
            touched[1] = to.to_usize();
            n_touched = 2;
        }

        Move::EnPassant { from, to } => {
            let piece = board.piece_at(from).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };
            let piece_type = role_index(Role::Pawn);

            remove_feature(&mut delta, side, from.to_usize(), piece_type, white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);

            let cap_sq = Square::from_coords(to.file(), from.rank()).to_usize();
            remove_feature(&mut delta, 1 - side, cap_sq, piece_type, white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);

            add_feature(&mut delta, side, to.to_usize(), piece_type, white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);

            // Keep track of touches squares to update threats
            touched[0] = from.to_usize();
            touched[1] = to.to_usize();
            touched[2] = cap_sq;
            n_touched = 3;

        }

        Move::Castle { king, rook } => {
            let piece = board.piece_at(king).unwrap();
            let side = if piece.color == Color::White { 0 } else { 1 };

            let king_from = king.to_usize();
            let rook_from = rook.to_usize();
            let kingside = rook.file() > king.file();
            let king_to = if kingside { king_from + 2 } else { king_from - 2 };
            let rook_to = if kingside { king_from + 1 } else { king_from - 1 };

            remove_feature(&mut delta, side, king_from, role_index(Role::King), white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);
            remove_feature(&mut delta, side, rook_from, role_index(Role::Rook), white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);
            add_feature(&mut delta, side, king_to, role_index(Role::King), white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);
            add_feature(&mut delta, side, rook_to, role_index(Role::Rook), white_bucket, white_is_mirrored,black_is_mirrored, black_bucket);

            touched[0] = king_from;
            touched[1] = king_to;
            touched[2] = rook_from;
            touched[3] = rook_to;
            n_touched = 4;
        }
        _ => {n_touched = 0;}
    }

    if is_king_move::<P>(mv) {
        let stm = pos.turn();

        let (new_bucket, new_is_mirrored) = get_bucket(child_pos.board(), stm);
        let (old_bucket, old_is_mirrored) = match stm {
            Color::White => (white_bucket, state.white_is_mirrored),
            Color::Black => (black_bucket, state.black_is_mirrored),
        };

        if new_bucket != old_bucket || new_is_mirrored != old_is_mirrored {
            match stm {
                Color::White => { state.white_bucket = new_bucket; state.white_is_mirrored = new_is_mirrored; }
                Color::Black => { state.black_bucket = new_bucket; state.black_is_mirrored = new_is_mirrored; }
            }
            delta.is_refresh = true;
            delta.refresh_color = stm;
        }
    }

    update_threats(&mut delta, state, child_pos, &touched[..n_touched]);
    state.stack.push(delta);
}

#[inline(always)]
pub fn unmake_move_nnue<P: Position>(pos: &P, net: &Network, state: &mut NNUEState) {
    let is_clean = state.applied == state.stack.len();
    let delta = state.stack.pop().unwrap();

    for i in 0..delta.n_threat_squares_affected as usize {
        let sq = delta.threat_squares_affected[i];
        state.threats[sq as usize] = attacks_from_for(pos.board(), Square::new(sq as u32));
    }

    if delta.is_refresh {
        let (old_bucket, old_is_mirrored) = get_bucket(pos.board(), delta.refresh_color);
        match delta.refresh_color {
            Color::White => { state.white_bucket = old_bucket;state.white_is_mirrored = old_is_mirrored;}
            Color::Black => { state.black_bucket = old_bucket;state.black_is_mirrored = old_is_mirrored; }
        }
    }

    if !is_clean {
        return;
    }
    state.applied -= 1;

    if delta.is_refresh {
        let stm = delta.refresh_color;

        match stm {
            Color::White => {
                state.black_acc.apply_feature_updates(&delta.black_removed[..delta.n_removed as usize], &delta.black_added[..delta.n_added as usize], net);
                state.black_acc.apply_feature_updates(&delta.threat_black_removed[..delta.n_threat_removed as usize], &delta.threat_black_added[..delta.n_threat_added as usize], net);
            }
            Color::Black => {
                state.white_acc.apply_feature_updates(&delta.white_removed[..delta.n_removed as usize], &delta.white_added[..delta.n_added as usize], net);
                state.white_acc.apply_feature_updates(&delta.threat_white_removed[..delta.n_threat_removed as usize], &delta.threat_white_added[..delta.n_threat_added as usize], net);
            }
        }

        let bucket = match stm {
            Color::White => state.white_bucket,
            Color::Black => state.black_bucket
        };

        match stm {
            Color::White => state.white_acc = finny_refresh(pos, net, Color::White, bucket, state.white_is_mirrored, &state.threats, &mut state.ft),
            Color::Black => state.black_acc = finny_refresh(pos, net, Color::Black, bucket, state.black_is_mirrored, &state.threats, &mut state.ft),
        }
        return;
    }

    state.white_acc.apply_feature_updates(&delta.white_removed[..delta.n_removed as usize], &delta.white_added[..delta.n_added as usize], net);
    state.black_acc.apply_feature_updates(&delta.black_removed[..delta.n_removed as usize], &delta.black_added[..delta.n_added as usize], net);
    state.white_acc.apply_feature_updates(&delta.threat_white_removed[..delta.n_threat_removed as usize], &delta.threat_white_added[..delta.n_threat_added as usize], net);
    state.black_acc.apply_feature_updates(&delta.threat_black_removed[..delta.n_threat_removed as usize], &delta.threat_black_added[..delta.n_threat_added as usize], net);
}



// =====================================================================================================================//
// HELPER FUNCTION TO ACTIVATE AND DEACTIVATE FEATURES INTO FUSED UPDATES                                               //
// =====================================================================================================================//
#[inline(always)]
fn remove_feature(delta: &mut AccumulatorDelta, side: usize, sq: usize, piece_type: usize, white_bucket: usize, white_is_mirrored : bool, black_is_mirrored : bool, black_bucket: usize) {
    let white_idx = calculate_index(side, sq, piece_type, Color::White, white_bucket,white_is_mirrored);
    let black_idx = calculate_index(side, sq, piece_type, Color::Black, black_bucket,black_is_mirrored);
    let n = delta.n_removed as usize;
    delta.white_removed[n] = white_idx;
    delta.black_removed[n] = black_idx;
    delta.n_removed += 1;
}
#[inline(always)]
fn add_feature(delta: &mut AccumulatorDelta, side: usize, sq: usize, piece_type: usize, white_bucket: usize, white_is_mirrored : bool, black_is_mirrored : bool, black_bucket: usize) {
    let white_idx = calculate_index(side, sq, piece_type, Color::White, white_bucket,white_is_mirrored);
    let black_idx = calculate_index(side, sq, piece_type, Color::Black, black_bucket,black_is_mirrored);
    let n = delta.n_added as usize;
    delta.white_added[n] = white_idx;
    delta.black_added[n] = black_idx;
    delta.n_added += 1;
}
#[inline(always)]
fn add_threat_feature(delta: &mut AccumulatorDelta, attacker: usize, target: usize, white_mirror: bool, black_mirror: bool) {
    let n = delta.n_threat_added as usize;
    delta.threat_white_added[n] = calculate_threat_index(attacker, target, Color::White, white_mirror);
    delta.threat_black_added[n] = calculate_threat_index(attacker, target, Color::Black, black_mirror);
    delta.n_threat_added += 1;
}
#[inline(always)]
fn remove_threat_feature(delta: &mut AccumulatorDelta, attacker: usize, target: usize, white_mirror: bool, black_mirror: bool) {
    let n = delta.n_threat_removed as usize;
    delta.threat_white_removed[n] = calculate_threat_index(attacker, target, Color::White, white_mirror);
    delta.threat_black_removed[n] = calculate_threat_index(attacker, target, Color::Black, black_mirror);
    delta.n_threat_removed += 1;
}
#[inline(always)]
fn add_affected_square(sq: usize, affected: &mut [usize; 64], n: &mut usize) {
    if !affected[..*n].contains(&sq) {
        affected[*n] = sq;
        *n += 1;
    }
}
#[inline(always)]
fn attacks_to_sq(board: &Board, sq: Square, occupied: Bitboard) -> Bitboard {
    (attacks::rook_attacks(sq, occupied) & board.rooks_and_queens())
        | (attacks::bishop_attacks(sq, occupied) & board.bishops_and_queens())
        | (attacks::knight_attacks(sq) & board.by_role(Role::Knight))
        | (attacks::king_attacks(sq) & board.by_role(Role::King))
        | (attacks::pawn_attacks(Color::White, sq) & board.by_role(Role::Pawn) & board.by_color(Color::Black))
        | (attacks::pawn_attacks(Color::Black, sq) & board.by_role(Role::Pawn) & board.by_color(Color::White))
}
#[inline(always)]
fn update_threats<P: Position>(delta: &mut AccumulatorDelta, state: &mut NNUEState, child_pos: &P, touched: &[usize]) {
    let board = child_pos.board();
    let new_occ = board.occupied();

    let mut affected = [0usize; 64];
    let mut n_affected = 0;

    for &sq in touched {
        add_affected_square(sq, &mut affected, &mut n_affected);
        let target_sq = Square::new(sq as u32);
        let attackers = attacks_to_sq(board,target_sq, new_occ);

        for att_sq in attackers {
            add_affected_square(att_sq.to_usize(), &mut affected, &mut n_affected);
        }
    }

    let white_mirror = state.white_is_mirrored;
    let black_mirror = state.black_is_mirrored;

    for i in 0..n_affected {
        let sq = affected[i];
        let old_attacks = state.threats[sq];
        let new_attacks = attacks_from_for(board, Square::new(sq as u32));

        if old_attacks == new_attacks {
            continue;
        }

        for target in old_attacks & !new_attacks {
            remove_threat_feature(delta, sq, target.to_usize(), white_mirror, black_mirror);
        }
        for target in new_attacks & !old_attacks {
            add_threat_feature(delta, sq, target.to_usize(), white_mirror, black_mirror);
        }

        state.threats[sq] = new_attacks;

        let n = delta.n_threat_squares_affected as usize;
        delta.threat_squares_affected[n] = sq as u16;
        delta.n_threat_squares_affected += 1;
    }
}
#[inline(always)]
fn get_bb(board: &Board) -> [[Bitboard; 6]; 2] {
    let mut bb = [[Bitboard::EMPTY; 6]; 2];
    let white = board.white();
    let black = board.black();
    for role in [Role::Pawn, Role::Knight, Role::Bishop, Role::Rook, Role::Queen, Role::King] {
        let role_bb = board.by_role(role);
        bb[0][role_index(role)] = role_bb & white;
        bb[1][role_index(role)] = role_bb & black;
    }
    bb
}