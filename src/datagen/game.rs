use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use shakmaty::{Chess, Position, Move, Color, EnPassantMode};
use rand::RngExt;
use shakmaty::zobrist::Zobrist64;
use crate::datagen::datagen_config::DatagenConfig;
use crate::datagen::datagen_format::RawPosition;
use crate::datagen::adjudication::{check_adjudication, filter_position, terminal_wdl, FilterResult};
use crate::datagen::book::EpdBook;
use crate::engine::corrhist::CorrectionHistoryTable;
use crate::engine::params::Params;
use crate::engine::search::context::{NNUEState};
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::search::search::search;
use crate::engine::tt::TranspositionTable;
use crate::engine::utility::build_search_context;
use crate::nnue::network::Network;
use crate::uci::state::UciState;
use crate::engine::history::HistoryTables;

const MATE_THRESHOLD: i16 = 29_500;

pub fn run_game(
    config:      &DatagenConfig,
    book:        Option<&EpdBook>,
    net:         &Network,
    params:      &Params,
    ordering:    &MoveOrdering,
    tt_0:        &TranspositionTable,
    tt_1:        &TranspositionTable,
    rng:         &mut impl rand::Rng,
) -> Vec<RawPosition> {

    let mut pos = match book {
        Some(b) => b.random_position(rng).unwrap_or_else(Chess::new),
        None    => Chess::new(),
    };

    let mut repetition_stack = Vec::new();
    let mut collected: Vec<RawPosition> = Vec::new();

    // ---------------------------------------------------------------- //
    // Random opening //
    // ---------------------------------------------------------------- //


    let random_plies = if config.random_opening_plies > 0 {rng.random_range(0..config.random_opening_plies)} else { 0 };
    let mut score_history: Vec<i32> = Vec::new(); // white-relative scores for adjudication

    for _ in 0..random_plies {
        let moves: Vec<Move> = pos.legal_moves().into_iter().collect();
        if moves.is_empty() { return vec![]; }
        let mv = moves[rng.random_range(0..moves.len())].clone();
        repetition_stack.push(pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0);
        pos.play_unchecked(mv);
    }

    if pos.legal_moves().is_empty() { return vec![]; }

    // ---------------------------------------------------------------- //
    // Game loop — play until natural game end                          //
    // ---------------------------------------------------------------- //

    let mut first_move = true;


    let wdl = loop {
        let legal_moves = pos.legal_moves();

        // --- Terminal conditions ---
        if legal_moves.is_empty() { break terminal_wdl(&pos);}
        if pos.is_insufficient_material() { break 0.5; }
        if pos.halfmoves() >= 100 { break 0.5;  }
        if is_threefold(&repetition_stack) { break 0.5; }

        // Pick network and TT for the side to move
        let (network, tt, net_id) = if pos.turn() == Color::White {
            (net, tt_0, 0u8)
        } else {
            (net, tt_1, 1u8)
        };



        let mut ctx = build_search_context(
            tt,
            CorrectionHistoryTable::new(256,32),
            CorrectionHistoryTable::new(256,32),
            CorrectionHistoryTable::new(256,0),
            CorrectionHistoryTable::new(256,0),
            HistoryTables::new(),
            params,
            ordering,
            network,
            repetition_stack.clone(),
            NNUEState::new(&pos, network),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
            false,
            None);


        let (score, best_move, _pv,_) = search(
            &pos,
            &mut ctx,
            &UciState::new(false),
            128,
            Some(Duration::from_secs(3600)),
            config.nodes_per_move,
        );

        // This little block makes sure the initial position from opening book + random plies is balanced.
        // Not where one player has just blundered a piece.

        if first_move && score.abs() > 150{
            break 0.5;
        }
        else {
            first_move = false;
        }

        let white_relative_score = if pos.turn() == Color::White { score } else { -score };
        score_history.push(white_relative_score);

        let nodes = (*ctx.node_count).load(Ordering::Relaxed);

        // Check adjudication
        if let Some(adj_wdl) = check_adjudication(&score_history, &config) {
            break adj_wdl;
        }



        // --- Filter and collect --- //
        if filter_position(&pos, &best_move, score as i16, MATE_THRESHOLD) == FilterResult::Keep {
            collected.push(RawPosition {
                fen:       pos_to_fen(&pos),
                score :    white_relative_score,
                wdl:       0.5, // filled in after game ends
                net_id:    net_id as u8,
                nodes,
                depth : ctx.stats.completed_depth,
                pawn_hash: 0,
            });
        }

        // --- Play move ---
        repetition_stack.push(pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0);
        pos.play_unchecked(best_move);
    };

    // Fill in WDL from each position's STM perspective
    for p in &mut collected {
        p.wdl = wdl;
    }

    collected
}

// ------------------------------------------------------------------ //
// Helpers                                                            //
// ------------------------------------------------------------------ //

/// Simple threefold repetition check against the repetition stack.
fn is_threefold(stack: &[u64]) -> bool {
    if stack.len() < 8 { return false; }
    let last = *stack.last().unwrap();
    stack.iter().filter(|&&h| h == last).count() >= 3
}

/// Serialize position to FEN string.
fn pos_to_fen(pos: &Chess) -> String {
    shakmaty::fen::Fen::from_position(
        &pos.clone(),
        EnPassantMode::Legal,
    ).to_string()
}

/// Cheap pawn-only hash for filtering training data
fn _pawn_hash(pos: &Chess) -> u64 {
    use shakmaty::{Role, Square};
    let mut hash: u64 = 0;
    for sq in Square::ALL {
        if let Some(piece) = pos.board().piece_at(sq) {
            if piece.role == Role::Pawn {
                let color_key: u64 = if piece.color == Color::White {
                    0x9e3779b97f4a7c15
                } else {
                    0x6c62272e07bb0142
                };
                hash ^= (sq as u64).wrapping_mul(color_key);
            }
        }
    }
    hash
}