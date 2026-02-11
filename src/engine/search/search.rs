
use std::time::{Duration, Instant};
use shakmaty::{Chess, Move, Position};
use crate::engine::constants::CHECKMATE_VALUE;
use crate::engine::eval::evaluate;
use crate::engine::params::Params;
use crate::engine::search::pv::PvTable;
use crate::engine::time_manager::compute_time_limit;

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
    stats: &mut SearchStats,
    max_depth: usize,
    time_remaining: Option<Duration>,
) -> (Move, f32) {
    let start = Instant::now();
    let total_time = compute_time_limit(time_remaining, Some(0));

    let mut pv = PvTable::new(max_depth);
    let mut best_score = f32::NEG_INFINITY;

    for depth in 1..=max_depth {
        if start.elapsed() > total_time {
            break;
        }

        pv.clear_from(0);

        best_score = negamax(
            pos,
            params,
            stats,
            &mut pv,
            depth,
            0,
            f32::NEG_INFINITY,
            f32::INFINITY,
        );
    }

    (
        pv.best_move().expect("no legal moves"),
        best_score,
    )
}


fn negamax(pos: &Chess, params : &Params, stats: &mut SearchStats, depth: usize, ply: i32, mut alpha: f32, beta: f32) -> f32 {
    stats.nodes +=1;
    stats.depth_sum += ply as u64;
    stats.depth_samples += 1;
    if pos.is_checkmate() {
        return -CHECKMATE_VALUE-depth as f32
    }
    if pos.is_stalemate() || pos.is_insufficient_material() {
        return 0.0
    }

    if depth == 0 {
        return evaluate(pos, params);
    }

    let mut score = 0.0;
    let mut best_score = f32::NEG_INFINITY;

    let move_list = pos.legal_moves();

    for i in 0..move_list.len() {
        let mv = &move_list[i];
        let mut child_pos = pos.clone();
        child_pos.play_unchecked(mv);
        score = -negamax(&child_pos, params,stats,depth - 1,ply+1, -beta, -alpha);

        if score > best_score {
            best_score = score;
        }
        if score >= beta{
            break;
        }
        if score > alpha {
            alpha = score;
        }
    }
    best_score

}




