use std::cmp;
use std::time::{Duration, Instant};
use shakmaty::{Chess, EnPassantMode, Move, Position};
use shakmaty::zobrist::{Zobrist64};
use crate::engine::eval::evaluate;

use crate::engine::search::context::SearchContext;

use crate::engine::time_manager::compute_time_limit;
use crate::engine::tt::Bound;
use crate::engine::types::{DRAW_SCORE, MATE_SCORE};


pub struct SearchStats {
    pub nodes: u64,
    pub depth_sum: u64,
    pub depth_samples: u64,
    pub seldepth: u32,
    pub duration: Duration,
}

impl SearchStats {
    pub(crate) fn default() -> SearchStats {
        Self {
            nodes: 0,
            depth_sum: 0,
            depth_samples: 0,
            seldepth: 0,
            duration: Duration::ZERO,
        }
    }
}

pub fn search(pos: &Chess, ctx: &mut SearchContext, max_depth: usize, time_remaining: Option<Duration>) -> f32 {

    let start = Instant::now();
    let total_time = compute_time_limit(pos, time_remaining, Some(Duration::from_millis(0)));

    let mut best_score = f32::NEG_INFINITY;


    for depth in 1..=max_depth {
        if start.elapsed() > total_time {
            break;
        }

        ctx.pv.clear_from(0);
        ctx.multipv.clear();

        let score = negamax(pos, ctx, depth, 0, f32::NEG_INFINITY, f32::INFINITY);
        best_score = score;
    }

    ctx.stats.duration = start.elapsed();

    best_score

}


#[inline(always)]
fn negamax(
    pos: &Chess,
    ctx: &mut SearchContext,
    mut depth: usize,
    ply: usize,
    mut alpha: f32,
    beta: f32,
) -> f32 {
    ctx.stats.nodes += 1;
    ctx.stats.seldepth = cmp::max(ply as u32, ctx.stats.seldepth);
    ctx.stats.depth_sum += ply as u64;
    ctx.stats.depth_samples += 1;

    ctx.pv.clear_from(ply);

    if pos.is_checkmate() {
        return -MATE_SCORE + ply as f32;

    }

    if ctx.is_threefold(pos) || ctx.is_50_moves(pos){
        return DRAW_SCORE;
    }

    if pos.is_stalemate() || pos.is_insufficient_material() {
        return DRAW_SCORE;
    }

    if pos.is_check() {
        depth += 1;
    }

    if depth == 0 {
        return quiescence(pos, ctx, alpha, beta);
    }

    let hash = pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

    if let Some(score) = tt_probe(hash, ctx, depth, alpha, beta) {
        return score;
    }

    let original_alpha = alpha;

    let mut best_score = f32::NEG_INFINITY;
    let mut best_move = None;

    let mut moves = pos.legal_moves();

    // is this correct?
    let pv_table = &ctx.pv.table;
    let pv_move: Option<Move> = pv_table.get(ply).and_then(|l| l.first()).cloned();

    let tt_move = tt_best_move(hash, ctx);

    ctx.ordering.order_moves(pos, pv_move.as_ref(), tt_move.as_ref(), &mut moves);


    for mv in moves {
        let mut child_pos = pos.clone();

        child_pos.play_unchecked(mv);
        let hash_child = child_pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

        ctx.increase_history(hash_child);

        let score = -negamax(&child_pos, ctx, depth - 1, ply + 1, -beta, -alpha);

        ctx.decrease_history();

        if score > best_score {
            best_score = score;
            best_move = Some(mv);
            update_pv(ply, mv, best_score, ctx);
        }

        if best_score >= beta {
            break;
        }

        if best_score > alpha {
            alpha = best_score;
        }
    }
    tt_store(hash, ctx, depth, best_score, original_alpha, beta, best_move,
    );
    best_score
}

#[inline(always)]
fn quiescence(
    pos: &Chess,
    ctx: &mut SearchContext,
    mut alpha: f32,
    beta: f32,
) -> f32 {
    ctx.stats.nodes += 1;


    if ctx.is_threefold(pos) || ctx.is_50_moves(pos){
        return DRAW_SCORE;
    }

    let hash = pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

    // TT probe for qsearch
    if let Some(score) = tt_probe(hash, ctx, 0, alpha, beta) {
        return score;
    }

    let stand_pat = evaluate(pos, ctx.params);

    if stand_pat >= beta {
        return beta;
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let original_alpha = alpha;

    let mut moves = pos.capture_moves();

    ctx.ordering.order_captures(pos, &mut moves);

    for mv in moves {
        let mut child = pos.clone();

        child.play_unchecked(mv);

        let child_hash = child.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

        ctx.increase_history(child_hash);

        let score = -quiescence(&child, ctx, -beta, -alpha);

        ctx.decrease_history();

        if score >= beta {
            return beta;
        }

        if score > alpha {
            alpha = score;
        }
    }
    tt_store(hash, ctx, 0, alpha, original_alpha, beta, None,
    );
    alpha
}


#[inline(always)]
fn update_pv(ply: usize, mv: Move, best_score: f32, ctx: &mut SearchContext) {
    let child_line = ctx.pv.table[ply + 1].clone();
    ctx.pv.set_pv(ply, mv, &child_line);

    if ply == 0 {
        ctx.multipv
            .insert(best_score, ctx.pv.pv_line().to_vec());
    }
}
#[inline(always)]
fn tt_probe(key : u64, ctx: &mut SearchContext, depth: usize, alpha: f32, beta: f32, ) -> Option<f32> {


    if let Some(entry) = ctx.tt.probe(key) {
        if entry.depth as usize >= depth {
            match entry.bound {
                Bound::Exact => return Some(entry.score),
                Bound::Lower if entry.score >= beta => {
                    return Some(entry.score)
                }
                Bound::Upper if entry.score <= alpha => {
                    return Some(entry.score)
                }
                _ => {}
            }
        }
    }
    None
}
#[inline(always)]
fn tt_store(key : u64, ctx: &mut SearchContext, depth: usize, best_score: f32, alpha: f32, beta: f32, best_move: Option<Move>, ) {
    let bound = if best_score <= alpha {
        Bound::Upper
    } else if best_score >= beta {
        Bound::Lower
    } else {
        Bound::Exact
    };
    ctx.tt.store(key, depth, best_score, bound, best_move);
}
#[inline(always)]
fn tt_best_move(key : u64, ctx: &mut SearchContext, ) -> Option<Move> {
    ctx.tt
        .probe(key)
        .and_then(|e| e.best_move.clone())
}