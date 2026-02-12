use std::time::{Duration, Instant};
use shakmaty::{Chess, Move, Position};

use crate::engine::eval::evaluate;
use crate::engine::params::Params;
use crate::engine::search::context::SearchContext;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::search::pv::{MultiPv, PvTable};
use crate::engine::time_manager::compute_time_limit;
use crate::engine::types::{DRAW_SCORE, MATE_SCORE};

#[derive(Default)]
pub struct SearchStats {
    pub nodes: u64,
    pub depth_sum: u64,
    pub depth_samples: u64,
    pub seldepth: u32,
    pub duration: Duration,
}

pub fn search(
    pos: &Chess,
    params: &Params,
    max_depth: usize,
    multipv_count: usize,
    time_remaining: Option<Duration>,
) -> (Move, f32, SearchStats, MultiPv) {
    let start = Instant::now();
    let total_time =
        compute_time_limit(pos, time_remaining, Option::from(Duration::from_millis(0)));

    let mut best_score = f32::NEG_INFINITY;
    let ordering = MoveOrdering::new(&params.piece_values);

    let mut ctx = SearchContext {
        params,
        ordering: &ordering,
        pv: PvTable::new(64),
        stats: SearchStats::default(),
        multipv: MultiPv::new(multipv_count),
    };

    for depth in 1..=max_depth {
        if start.elapsed() > total_time {
            break;
        }

        ctx.pv.clear_from(0);
        ctx.multipv.clear();

        let score = negamax(pos, &mut ctx, depth, 0, f32::NEG_INFINITY, f32::INFINITY);
        best_score = score;
    }

    ctx.stats.duration = start.elapsed();

    (
        ctx.pv.best_move().expect("no legal moves"),
        best_score,
        ctx.stats,
        ctx.multipv,
    )
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
    ctx.stats.seldepth = ctx.stats.seldepth.max(ply as u32);
    ctx.stats.depth_sum += ply as u64;
    ctx.stats.depth_samples += 1;

    ctx.pv.clear_from(ply);

    if pos.is_checkmate() {
        return -(MATE_SCORE as f32) + ply as f32;
    }

    if pos.is_stalemate() || pos.is_insufficient_material() {
        return DRAW_SCORE as f32;
    }

    if pos.is_check() {
        depth += 1;
    }

    if depth == 0 {
        return quiescence(pos, ctx, alpha, beta);
    }

    let mut best_score = f32::NEG_INFINITY;
    let mut moves = pos.legal_moves();

    let pv_move = ctx.pv.table.get(ply).and_then(|l| l.first());

    ctx.ordering.order_moves(pos, pv_move, &mut moves);



    for mv in moves {
        let mut child_pos = pos.clone();
        child_pos.play_unchecked(&mv);

        let score = -negamax(&child_pos, ctx, depth - 1, ply + 1, -beta, -alpha);

        if score > best_score {
            best_score = score;
            update_pv(ply, mv, best_score, ctx);
        }

        if best_score >= beta {
            break;
        }

        if best_score > alpha {
            alpha = best_score;
        }
    }

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

    let stand_pat = evaluate(pos, ctx.params);

    if stand_pat >= beta {
        return beta;
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let mut moves = pos.capture_moves();
    ctx.ordering.order_captures(pos, &mut moves);

    for mv in moves {
        let mut child = pos.clone();
        child.play_unchecked(&mv);

        let score = -quiescence(&child, ctx, -beta, -alpha);

        if score >= beta {
            return beta;
        }

        if score > alpha {
            alpha = score;
        }
    }

    alpha
}

#[inline(always)]
fn update_pv(ply: usize, mv: Move, best_score: f32, ctx: &mut SearchContext) {
    let child_line = ctx.pv.table[ply + 1].clone();
    ctx.pv.set_pv(ply, mv.clone(), &child_line);

    if ply == 0 {
        ctx.multipv
            .insert(best_score, ctx.pv.pv_line().to_vec());
    }
}
