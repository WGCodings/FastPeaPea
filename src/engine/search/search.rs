use std::time::{Duration, Instant};
use chess::{Board, ChessMove, MoveGen, BoardStatus, Piece};
use chess::BoardStatus::{Checkmate, Stalemate};
use crate::engine::eval::evaluate;
use crate::engine::params::Params;
use crate::engine::search::context::SearchContext;
use crate::engine::search::ordering::{is_capture, MoveOrdering};
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
    board: &Board,
    params: &Params,
    max_depth: usize,
    multipv_count: usize,
    time_remaining: Option<Duration>,
) -> (ChessMove, f32, SearchStats, MultiPv) {

    let start = Instant::now();
    let total_time = compute_time_limit(board, time_remaining, Some(Duration::ZERO));

    let ordering = MoveOrdering::new(&params.piece_values);

    let mut ctx = SearchContext {
        params,
        ordering: &ordering,
        pv: PvTable::new(64),
        stats: SearchStats::default(),
        multipv: MultiPv::new(multipv_count),
    };

    let mut best_score = f32::NEG_INFINITY;

    for depth in 1..=max_depth {
        if start.elapsed() > total_time {
            break;
        }

        ctx.pv.clear_from(0);
        ctx.multipv.clear();

        let score = negamax(board, &mut ctx, depth, 0, f32::NEG_INFINITY, f32::INFINITY);
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
    board: &Board,
    ctx: &mut SearchContext,
    mut depth: usize,
    ply: usize,
    mut alpha: f32,
    beta: f32,
) -> f32 {

    ctx.stats.nodes += 1;
    ctx.stats.depth_sum += ply as u64;
    ctx.stats.depth_samples += 1;
    ctx.stats.seldepth = ctx.stats.seldepth.max(ply as u32);



    match board.status() {
        Checkmate => return -(MATE_SCORE as f32) + ply as f32,
        Stalemate => return DRAW_SCORE as f32,
        _ => {}
    }
    ctx.pv.clear_from(ply+1);


    if board.checkers().popcnt() > 0 {
        depth += 1;
    }

    if depth == 0 {
        return quiescence(board, ctx, alpha, beta);
    }

    let mut best_score = f32::NEG_INFINITY;

    let mut moves: Vec<ChessMove> = MoveGen::new_legal(board).collect();

    let pv_move = ctx.pv.table.get(ply).and_then(|l| l.first());


    ctx.ordering.order_moves(board, pv_move, &mut moves);

    for mv in moves {
        let child = board.make_move_new(mv);
        let score = -negamax(&child, ctx, depth - 1, ply + 1, -beta, -alpha);

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
    board: &Board,
    ctx: &mut SearchContext,
    mut alpha: f32,
    beta: f32,
) -> f32 {

    ctx.stats.nodes += 1;

    let stand_pat = evaluate(board, ctx.params);

    if stand_pat >= beta {
        return beta;
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    // Generate tactical moves only
    let mut moves: Vec<ChessMove> =
        MoveGen::new_legal(board)
            .filter(|m| is_capture(board, m))
            .collect();

    ctx.ordering.order_moves(board, None, &mut moves);

    for mv in moves {
        let child = board.make_move_new(mv);
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
fn update_pv(ply : usize, mv: ChessMove,best_score: f32,ctx: &mut SearchContext) {
    let child_line = ctx.pv.table[ply + 1].clone();
    ctx.pv.set_pv(ply, mv.clone(), &child_line);

    // ---- MULTI-PV (ROOT ONLY) ----
    if ply == 0 {
        ctx.multipv.insert(best_score, ctx.pv.pv_line().to_vec());
    }
}

