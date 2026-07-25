use std::cmp;
use std::sync::atomic::Ordering;

use std::time::{Duration, Instant};
use shakmaty::{Bitboard, Chess, EnPassantMode, Move, Position};
use shakmaty::zobrist::{Zobrist64};
use crate::engine::eval::evaluate;
use crate::engine::search::context::{make_move_nnue, make_null_move_nnue, unmake_move_nnue, unmake_null_move_nnue, SearchContext};
use crate::engine::search::pv::PvTable;
use crate::engine::search::see::see;
use crate::engine::time_manager::TimeManager;
use crate::engine::tt::{ encode_move, score_from_tt, tt_store, validate_move, Bound};
use crate::engine::types::{DRAW_SCORE, MATE_SCORE, MAX_INF, MIN_INF};
use crate::engine::utility::{print_search_info};
use crate::uci::state::UciState;

#[derive(Clone, Copy)]
pub struct SearchStats {
    pub nodes: u64,
    pub depth_sum: u64,
    pub depth_samples: u64,
    pub seldepth: u32,
    pub completed_depth: usize,
    pub duration: Duration,
    pub singular_extensions : u32
}

impl SearchStats {
    pub(crate) fn default() -> SearchStats {
        Self {
            nodes: 0,
            depth_sum: 0,
            depth_samples: 0,
            seldepth: 0,
            completed_depth: 0,
            duration: Duration::ZERO,
            singular_extensions: 0,
        }
    }
}

pub fn search(pos: &Chess, ctx: &mut SearchContext, uci : &UciState, max_depth: usize, time_remaining: Option<Duration>, max_nodes : u64) -> (i32, Move, Vec<Option<Move>>,SearchStats) {
    let start_time = Instant::now();
    let base_time = time_remaining.unwrap();

    ctx.time_limit = base_time;
    ctx.node_limit = max_nodes;

    ctx.start_time = start_time;
    (*ctx.stop).store(false, Ordering::Relaxed);

    ctx.tt.increment_age(); // TODO put this after search has finished,no?

    let mut tm = TimeManager::new(base_time, start_time);
    let mut best_score = MIN_INF;
    let mut best_move = None;
    let mut pv = PvTable::new();
    let mut latest_pv;
    let mut prev_score: i32 = 0;
    let mut avg_score = 0;
    let mut tt_pv = vec![];


    for depth in 1..=max_depth {
        pv.clear();

        if tm.should_stop() && depth > 1 { break; }
        
        let score = if depth >= ctx.params.aspw_min_depth as usize{
            aspiration_search(pos, ctx, depth, prev_score, avg_score, &mut pv)
        } else {
            negamax(pos, ctx, depth, 0, MIN_INF, MAX_INF, true, false, &mut pv)
        };

        if (*ctx.stop).load(Ordering::Relaxed) && depth >1 { break; }

        tm.update(score, pv.best_move());
        
        ctx.time_limit = tm.current_limit;

        best_score = score;
        prev_score = score;
        avg_score = (avg_score * (depth as i32-1) + score) / depth as i32;
        best_move = pv.best_move();
        latest_pv = pv; // why use pv table if we get pv from tt? verification?
        ctx.stats.completed_depth = depth;

        if ctx.is_main{
            tt_pv = print_search_info(ctx, uci, pos, depth, best_score, tm.elapsed(), latest_pv);
        }

    }

    ctx.stats.duration = tm.elapsed();

    // Age history tables
    ctx.corrhist_pawn.age_entries();
    ctx.corrhist_material.age_entries();
    ctx.corrhist_minor.age_entries();
    ctx.corrhist_major.age_entries();
    
    (best_score, best_move.unwrap(), tt_pv, ctx.stats)
}



#[inline(always)]
pub fn negamax(
    pos: &Chess,
    ctx: &mut SearchContext,
    mut depth: usize,
    ply: usize,
    mut alpha: i32,
    mut beta: i32,
    do_null : bool,
    cut_node : bool,
    pv: &mut PvTable,
) -> i32 {
    (*ctx.node_count).fetch_add(1, Ordering::Relaxed);
    ctx.stats.nodes += 1;
    ctx.stats.seldepth = cmp::max(ply as u32, ctx.stats.seldepth);
    ctx.stats.depth_sum += ply as u64;
    ctx.stats.depth_samples += 1;

    let is_root = ply == 0;
    let in_check = pos.is_check();
    let is_pv = beta-alpha >1;
    let is_excluded = ctx.excluded_move[ply].is_some();
    let mut do_probcut = true;


    let mut best_score = MIN_INF;
    let mut corr = 0;
    let mut best_move = None;

    if ply > 0 {
        ctx.stack.double_exts[ply] = ctx.stack.double_exts[ply-1];
    }

    check_time(ctx);

    if !is_root{
        if ctx.is_threefold(pos) || ctx.is_50_moves(pos)  || pos.is_insufficient_material() {
            return DRAW_SCORE;
        }

        // mate distance pruning
        alpha = alpha.max(ply as i32- MATE_SCORE);
        beta = beta.min(MATE_SCORE - ply as i32 - 1);
        if alpha >= beta {
            return alpha;
        }

        if in_check{
            depth += 1;
        }
    }

    if (*ctx.stop).load(Ordering::Relaxed){ return DRAW_SCORE; }


    if depth <= 0 {
        (*ctx.node_count).fetch_sub(1, Ordering::Relaxed);
        ctx.stats.nodes -= 1;
        return quiescence(pos, ctx, alpha, beta,ply);
    }

    let hash = pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;
    let mut score;

    // =====================================================================================================================//
    // TT PROBE                                                                                                             //
    // =====================================================================================================================//

    let tt_entry = if !is_excluded {
        ctx.tt.probe(hash)
    } else {
        None
    };

    if !is_root && !is_excluded && tt_entry.is_some() {
        if let Some(score) = tt_entry.as_ref().and_then(|e| e.try_score(depth, alpha, beta, ply)) {
            return score;
        }
    }

    let raw_eval = if is_excluded {
        ctx.stack.evals[ply]
    } else if let Some(e) = &tt_entry {
        e.eval
    } else {
        let eval = evaluate(pos, ctx.network, &mut ctx.nnue);
        tt_store(hash, ctx, depth, MIN_INF, eval, Bound::Upper, None ,ply);
        eval

    };

    let static_eval = if is_excluded {
        raw_eval
    } else {
        corr = ctx.corrhist_pawn.correct_evaluation(pos) + ctx.corrhist_material.correct_evaluation(pos);
        raw_eval + corr
    };


    ctx.stack.evals[ply] = static_eval;

    // Safety
    if ply > 63{
        return static_eval;
    }

    ctx.clear_killers_at(ply+1);


    let do_pruning = minors_or_majors(pos).count() >0 && !is_excluded;
    let improving = ctx.is_improving(ply);

    let mut can_futility_prune = false;


    // =====================================================================================================================//
    // REVERSE FUTILITY PRUNING                                                                                             //
    // =====================================================================================================================//
    let futility = (ctx.params.rfp_scaling as usize* depth) as i32 + ctx.params.rfp_improving_scaling * !improving as i32;
    if do_pruning && !is_pv && !in_check && depth <= ctx.params.rfp_max_depth as usize && !is_root && static_eval - futility   >=beta {
        return (static_eval + beta)/2;
    }

    // =====================================================================================================================//
    // STATIC NULL MOVE PRUNING                                                                                             //
    // =====================================================================================================================//
    if  do_pruning && !in_check && !is_pv && !is_mate_score(beta) {
        let score_margin = ctx.params.snmp_scaling * depth as i32;
        if static_eval-score_margin >= beta {
            return static_eval-score_margin
        }
    }

    // =====================================================================================================================//
    // NULL MOVE PRUNING                                                                                                    //
    // =====================================================================================================================//
    let nmp_margin : i32 = -ctx.params.nmp_margin + ctx.params.nmp_scaling * depth as i32 + ctx.params.nmp_improving_scaling * improving as i32 ;

    if  do_pruning && !in_check && !is_pv &&
        static_eval + nmp_margin >= beta &&
        do_null && !is_root && cut_node &&
        depth >=ctx.params.nmp_min_depth as usize {

        // TODO add term (static_eval-beta)/divisor to reduction formula
        // TODO add cutnode condition

        let mut reduction = (ctx.params.nmp_base_reduction as usize + depth/ctx.params.nmp_reduction_scaling as usize).min(depth);

        reduction += 2*improving as usize;

        reduction = reduction.clamp(1,depth);

        make_null_move_nnue(&mut ctx.nnue);

        let child_pos = pos.clone().swap_turn().unwrap();

        let hash_child = child_pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;
        ctx.increase_history(hash_child);

        let score = -negamax(&child_pos, ctx, depth - reduction, ply + 1, -beta, -beta + 1, false, false, &mut PvTable::new());

        //TODO add verification

        unmake_null_move_nnue(&mut ctx.nnue);

        ctx.decrease_history();

        if (*ctx.stop).load(Ordering::Relaxed){ return DRAW_SCORE;}

        if score >= beta  {

            if !is_mate_score(score){
                return beta;
            }

            /*

            if depth < ctx.params.nmp_verif_depth as usize && beta < MATE_SCORE - 128{
                if score > MATE_SCORE - 128{
                    return beta
                }
                return score;
            }
            let v_score = negamax(&child_pos, ctx, depth - reduction, ply + 1, beta-1, beta, false, &mut PvTable::new());

            if v_score >= beta {
                return v_score;
            }

             */

        }
    }


    // =====================================================================================================================//
    // RAZORING                                                                                                             //
    // =====================================================================================================================//
    if  do_pruning && !in_check && !is_pv
        && depth <= ctx.params.raz_max_depth as usize
        && static_eval + ctx.params.raz_thr *(depth as i32)  + improving as i32 * ctx.params.raz_improving_margin < alpha
    {
        let razor_score = quiescence(pos,ctx,alpha,beta,ply);
        if razor_score <= alpha{
            return razor_score;
        }
    }

    // =====================================================================================================================//
    // FUTILITY PRUNING PART 1                                                                                              //
    // =====================================================================================================================//
    if  depth <= ctx.params.fp_max_depth as usize && !is_pv && !in_check && !is_mate_score(alpha) && !is_mate_score(beta) && !is_excluded{
        let margin = ctx.params.fp_base+ depth as i32 * ctx.params.fp_scaling + ctx.params.fp_improving_margin * improving as i32;
        can_futility_prune = static_eval+margin <= alpha;
    }

    let mut moves = pos.legal_moves();


    // best move is encoded in the tt as 16 bits but does not contain all info,
    // so when we need it we reconstruct the tt move based on all available legal moves
    let tt_move = tt_entry.as_ref().and_then(|e| {
        let encoded = encode_move(e.best_move);
        if encoded == 0 { return None; }
        validate_move(encoded,&moves)
    });


    // =====================================================================================================================//
    // INTERNAL ITERATIVE REDUCTION                                                                                         //
    // =====================================================================================================================//
    // TODO try extra reduction of larger depth
    // TODO add cutnode condition
    // TODO There is also an idea of reducing depth by more if the node has been searched before at a depth greater than or equal to current depth, but yet no hash move is found.
    if tt_move.is_none() && depth >= ctx.params.iir_min_depth as usize{
        depth -= 1;
    }

    // =====================================================================================================================//
    // PROBCUT                                                                                                              //
    // =====================================================================================================================//
    // TODO optimise, filter captures from moves above and calculate SEE once
    // TODO experiment with parameters, maybe add improving heuristic later
    // TODO add cutnode condition

    let probcut_beta = beta + ctx.params.pc_beta_margin - i32::from(improving) * ctx.params.pc_improving_margin;

    do_probcut = tt_entry.as_ref().is_none_or(|e| {e.score >= probcut_beta});


    if !is_excluded
        && !is_pv
        && !in_check
        && depth >= ctx.params.pc_min_depth as usize
        && !is_mate_score(beta)
        && do_probcut
        && cut_node
        && false
    {
        let probcut_depth = (depth - 3 - ((static_eval-beta)/ctx.params.pc_depth_divisor) as usize)
            .clamp(0,ctx.params.pc_min_depth as usize)
            .clamp(0,depth-1);

        let mut captures = pos.capture_moves();
        ctx.ordering.order_captures(pos, &mut captures);

        for mv in captures {

            // TODO only do probcut for captures with high histroy?

            if see(pos,mv) as i32 <= ctx.params.pc_see_thr {
                continue;
            }

            let mut child_pos = pos.clone();

            child_pos.play_unchecked(mv);

            // TODO try skip if child pos is in check

            make_move_nnue(pos, &mv, &mut ctx.nnue);

            ctx.stack.moves[ply] = Some(mv);

            let hash_child = child_pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

            ctx.increase_history(hash_child);

            let mut probcut_score = -quiescence(&child_pos, ctx, -probcut_beta, -probcut_beta + 1, ply + 1);



            if probcut_score >= probcut_beta {
                probcut_score = -negamax(&child_pos, ctx, probcut_depth, ply + 1, -probcut_beta, -probcut_beta + 1, false, false, &mut PvTable::new());
            }

            ctx.decrease_history();

            unmake_move_nnue(ctx.network, &mut ctx.nnue);

            ctx.stack.moves[ply] = None;

            if probcut_score >= probcut_beta {
                tt_store(hash, ctx, probcut_depth + 1, probcut_score, raw_eval, Bound::Lower, Some(mv), ply);
                return probcut_score;
            }
        }
    }

    // =====================================================================================================================//
    // SINGULAR EXTENSION INFO                                                                                              //
    // =====================================================================================================================//

    let se_info: Option<(Move, i32)> = if !is_root && !is_excluded && depth >= ctx.params.se_min_depth as usize{
        tt_move.as_ref().and_then(|tt_mv| {
            tt_entry.as_ref().and_then(|entry| {
                let tt_depth_ok = entry.depth as usize + ctx.params.se_depth_ok as usize>= depth;
                let tt_score = score_from_tt(entry.score, ply);
                let not_mate = !is_mate_score(tt_score.abs());
                let is_lower_bound = matches!(entry.bound, Bound::Lower | Bound::Exact);

                if tt_depth_ok && not_mate && is_lower_bound {
                    Some((tt_mv.clone(), tt_score))
                } else {
                    None
                }
            })
        })
    } else {
        None
    };


    let mut moves_searched : i32 = 0;
    let mut local_pv = PvTable::new();
    let mut quiets_searched: Vec<Move> = Vec::new();
    let mut tacticals_searched: Vec<Move> = Vec::new();
    let mut node_type = Bound::Upper;

    ctx.ordering.order_moves(pos, ctx,  tt_move.as_ref(), &ctx.killers[ply], ply, &mut moves);


    for mv in moves {
        // Skip excluded move (used during singular extension search)
        if ctx.excluded_move[ply] == Some(mv) {
            continue;
        }

        let see= see(pos,mv);

        local_pv.clear();

        let is_capture = mv.is_capture();
        let is_quiet = !is_capture && !mv.is_promotion() ;


        moves_searched +=1;

        // TODO TEST IF PROMOTIONS ARE BEST ADDED TO QUIETS OR TACTICALS
        // Punish bad quiets and tacticals
        if is_quiet {
            // penalize quiets that fail low
            quiets_searched.push(mv);
        }
        // malus for captures that did not fail high
        if !is_quiet {
            tacticals_searched.push(mv);
        }

        // =====================================================================================================================//
        // LATE MOVE PRUNING                                                                                                    //
        // =====================================================================================================================//
        // TODO add history to tune threshold
        let lmp_moves= ctx.params.lmp_base +depth as i32 * ctx.params.lmp_lin_scaling + depth as i32 * depth as i32 * ctx.params.lmp_quad_scaling;
        if depth <= ctx.params.lmp_max_depth as usize
            && !is_pv
            && !in_check
            && moves_searched > lmp_moves
            && is_quiet {
            if see <= 0 {
                continue;
            }
        }
        // =====================================================================================================================//
        // QUIET HISTORY PRUNING                                                                                                      //
        // =====================================================================================================================//
        // TODO FINETUNE PARAMETERS TO MAKE IT WORK ADD CAPTURE HISTORY PRUNING WITH LARGER MARGIN
        if !in_check
            && !is_pv
            && depth <= ctx.params.hist_prune_depth as usize
            && moves_searched > 1  // never prune first move?
        {
            if is_quiet{
                let hist = ctx.history.quiet.get(pos, &mv) + ctx.history.continuation.get(&mv, ply, &ctx.stack.moves);
                if hist < -(ctx.params.hist_prune_margin * depth as i32) {
                    continue;
                }
            }
        }


        // =====================================================================================================================//
        // FUTILITY PRUNING PART 2                                                                                              //
        // =====================================================================================================================//
        if can_futility_prune && moves_searched >ctx.params.fp_min_moves_searched && is_quiet{
            continue;
        }


        // =====================================================================================================================//
        // HANGING PIECE PRUNING                                                                                                //
        // Prunes bad captures and quiet moves that hang pieces. It feels a little dangerous but it works so I leave it for now //
        // To be tuned later.                                                                                                   //
        // =====================================================================================================================//
        // From WIKI : This is usually done with a linear depth margin for captures, and a quadratic depth margin for quiets, though such details may vary.

        if !is_pv && !is_root && !in_check && !is_quiet{
            if (see as i32) <  - ctx.params.hpp_tactical_scaling * depth as i32  {
                continue;
            }
        }
        if !is_pv && !is_root && !in_check && is_quiet{
            if (see as i32) <  - ctx.params.hpp_quiet_scaling * depth as i32 * depth as i32  {
                continue;
            }
        }

        // =========================================================
        // SINGULAR EXTENSIONS (Thanks to Simbelmyne)
        // =========================================================
        let mut extension: i32 = 0;


        if let Some((ref se_mv, tt_score)) = se_info {
            if *se_mv == mv  {

                let mut se_pv = PvTable::new();

                let se_beta = (tt_score - ctx.params.se_scaling * depth as i32).max(-MATE_SCORE);
                let se_depth = (depth - 1) / 2;



                ctx.excluded_move[ply] = Some(mv);
                let se_score = negamax(pos, ctx, se_depth, ply, se_beta - 1, se_beta, false, cut_node, &mut se_pv);
                ctx.excluded_move[ply] = None;


                if (*ctx.stop).load(Ordering::Relaxed) { return DRAW_SCORE; }

                if se_score < se_beta  {
                    ctx.stats.singular_extensions += 1;
                    extension += 1;

                    if !is_pv
                        && se_score + (ctx.params.se_dext_margin) < se_beta
                        && ctx.stack.double_exts[ply] <= ctx.params.se_max_nr_dext
                    {
                        // Double extensions
                        extension += 1;
                        ctx.stack.double_exts[ply] += 1;
                        // Triple extensions
                        if is_quiet && se_score + (ctx.params.se_text_margin) < se_beta{
                            extension += 1;
                        }

                    }
                } else if se_beta >= beta{
                    return se_beta;
                }
                else if tt_score >= beta {
                    extension -= 1;
                }
                /*
                        
                else if cut_node {
                    extension -= 2;
                }
                */
            }
        }

        let mut child_pos = pos.clone();
        child_pos.play_unchecked(mv);



        make_move_nnue(pos, &mv, &mut ctx.nnue);

        let hash_child = child_pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

        ctx.increase_history(hash_child);

        // Push move into stack for continuation history
        ctx.stack.moves[ply] = Some(mv);

        // =====================================================================================================================//
        // START PVS SEARCH WITH FULL WINDOW FOR THE FIRST MOVE AND LMR FOR LATE MOVES                                          //
        // =====================================================================================================================//

        let extended_depth = (depth as i32 + extension).max(0) as usize;
        if moves_searched == 1{
            score = -negamax(&child_pos, ctx, extended_depth - 1, ply + 1, -beta, -alpha, true, !is_pv && !cut_node, &mut local_pv);
        }
        else {
            let mut reduction : i32;
            let mut red_clamped : usize = 0;

            if moves_searched >=ctx.params.lmr_min_searches && depth >= ctx.params.lmr_min_depth as usize && !in_check && is_quiet{

                // Base reductionQ
                reduction = (ctx.params.lmr_red_constant+(depth as f32).ln() * (moves_searched as f32).ln()/ctx.params.lmr_red_scaling) as i32;

                // TODO increase reduction if not improving/cut node/expected cut node
                // TODO decrease if passed pawn push

                if let Some(ttm) = tt_move {
                    if ttm.is_capture() || ttm.is_promotion() {
                        reduction += 1;
                    }
                }

                reduction += i32::from(!improving);

                reduction += i32::from(cut_node);

                reduction -= corr.abs()/ctx.params.lmr_corr_scaling;


                if see as i32 <= ctx.params.lmr_see_thr {
                    reduction += 1;
                }

                if child_pos.is_check(){
                    reduction -=1;
                }
                if is_pv{
                    reduction -= 1;
                }

                let hist_red = (ctx.history.quiet.get(pos, &mv) + ctx.history.continuation.get(&mv, ply, &ctx.stack.moves))/ (ctx.params.lmr_history_divisor);

                reduction -= hist_red;

                red_clamped = (reduction.max(0) as usize).clamp(0,extended_depth - 1);

            }

            score = -negamax(&child_pos, ctx, (extended_depth - 1 - red_clamped).max(0) , ply + 1, -alpha-1, -alpha, true, true, &mut local_pv);

            if score > alpha && red_clamped >0 {
                score = -negamax(&child_pos, ctx, (extended_depth - 1).max(0), ply + 1, -alpha - 1, -alpha, true, !cut_node, &mut local_pv);
            }
            if score > alpha && score < beta {
                score = -negamax(&child_pos, ctx, (extended_depth - 1).max(0), ply + 1, -beta, -alpha, true, false, &mut local_pv);
            }
        }

        // Pop move from stack
        ctx.stack.moves[ply] = None;

        ctx.decrease_history();
        unmake_move_nnue(ctx.network, &mut ctx.nnue);

        if score > best_score {
            best_score = score;
            best_move = Some(mv);
        }

        if score >= beta {
            let bonus = ctx.params.cont_hist_scaling * depth as i32 - ctx.params.cont_hist_base;
            let malus = ctx.params.cont_hist_malus_scaling * depth as i32 - ctx.params.cont_hist_malus_base;
            node_type = Bound::Lower;

            if !is_capture{

                ctx.store_killer(ply, mv);

                ctx.history.quiet.update(pos,&mv,bonus,malus,&quiets_searched); // Update quiet history, bonus for move, malus for quiets searched

                ctx.history.continuation.update(ply, &mv, bonus, malus, &quiets_searched, &ctx.stack.moves); // Update continuation history, bonus for move, malus for quiets searched

            }else {
                ctx.history.noisy.update(pos,&mv,bonus,malus,&tacticals_searched);
            }

            break;
        }
        if score > alpha {
            alpha = score;
            pv.add_child_to_parent(mv,&local_pv);
            node_type = Bound::Exact;

        }


    }

    if moves_searched == 0{
        return i32::from(in_check) * (-MATE_SCORE + ply as i32);
    }


    if !is_excluded && !in_check && !best_move.is_some_and(|mv| mv.is_capture() || mv.is_promotion())
        && !(node_type == Bound::Lower && best_score <= static_eval)
        && !(node_type == Bound::Upper && best_score >= static_eval) {
        ctx.corrhist_pawn.update_correction_history(pos, depth as i32, best_score - static_eval);
        ctx.corrhist_material.update_correction_history(pos, depth as i32, best_score - static_eval);
    }


    if !(*ctx.stop).load(Ordering::Relaxed) && !is_excluded{
        tt_store(hash, ctx, depth, best_score, raw_eval,node_type, best_move,ply);
    }
    best_score
}

// =====================================================================================================================//
// Q SEARCH                                                                                                             //
// =====================================================================================================================//

#[inline(always)]
pub fn quiescence(
    pos: &Chess,
    ctx: &mut SearchContext,
    mut alpha: i32,
    beta: i32,
    ply : usize
) -> i32 {

    ctx.stats.nodes += 1;
    (*ctx.node_count).fetch_add(1, Ordering::Relaxed);
    check_time(ctx);
    if (*ctx.stop).load(Ordering::Relaxed){ return DRAW_SCORE; }

    if ctx.is_threefold(pos) || ctx.is_50_moves(pos) {
        return DRAW_SCORE;
    }

    let in_check = pos.is_check();
    let hash = pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

    // TT probe for qsearch
    let tt_entry = ctx.tt.probe(hash);

    if tt_entry.is_some() {
        if let Some(score) = tt_entry.as_ref().and_then(|e| e.try_score(0, alpha, beta, ply)) {
            return score;
        }
    }

    // TODO if in check, generate all evasions, not only capture evasions
    let raw_eval = if in_check{
        -MATE_SCORE + ply as i32
    }
    else if let Some(entry) = &tt_entry {
        entry.eval
    } else {
        let eval = evaluate(pos, ctx.network, &mut ctx.nnue);
        //tt_store(hash, ctx, 0, MIN_INF, eval, Bound::Upper, None ,ply);
        eval

    };

    let static_eval = if in_check{
        -MATE_SCORE + ply as i32
    } else{
        raw_eval
            + ctx.corrhist_pawn.correct_evaluation(pos)
            + ctx.corrhist_material.correct_evaluation(pos)
    };

   if static_eval >= beta {
        return static_eval;
    }

    if static_eval > alpha {
        alpha = static_eval;
    }

    let mut node_type = Bound::Upper;
    //let mut moves_searched = 0;


    let mut moves = pos.capture_moves();

    ctx.ordering.order_captures(pos, &mut moves);

    for mv in moves {

        //moves_searched+=1;

        let see = see(pos, mv) as i32;

        if see <0{
            continue;
        }

        make_move_nnue(pos, &mv, &mut ctx.nnue);

        let mut child = pos.clone();

        child.play_unchecked(mv);

        let child_hash = child.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0;

        ctx.increase_history(child_hash);

        let score = -quiescence(&child, ctx, -beta, -alpha,ply+1);

        ctx.decrease_history();

        unmake_move_nnue(ctx.network, &mut ctx.nnue);

        if score >= beta {
            //node_type = Bound::Lower;
            // TODO try return score here
            return beta;
        }

        if score > alpha {
            node_type = Bound::Exact;
            alpha = score;
        }
    }
    /*
    if in_check && moves_searched==0{
        return -MATE_SCORE+ply as i32;
    }
    */
    if !(*ctx.stop).load(Ordering::Relaxed) {
        tt_store(hash, ctx, 0, alpha, raw_eval,node_type,None,ply);
    }
    alpha
}

// =====================================================================================================================//
// ASPIRATION SEARCH                                                                                                    //
// =====================================================================================================================//

#[inline(always)]
fn aspiration_search(pos: &Chess, ctx: &mut SearchContext, max_depth: usize, prev_score: i32, avg_score : i32, pv: &mut PvTable) -> i32 {
    let mut window = ctx.params.aspw_window_size + avg_score.abs()/50;
    let mut alpha = prev_score - window;
    let mut beta = prev_score + window;
    let mut depth = max_depth;

    loop {
        let score = negamax(pos, ctx, depth, 0, alpha, beta, true, false, pv);

        if (*ctx.stop).load(Ordering::Relaxed) { return score; }

        if score <= alpha {
            beta = (alpha + beta) / 2;
            alpha = cmp::max(alpha - window, MIN_INF);
            depth = max_depth;
        } else if score >= beta {
            beta = cmp::min(beta + window, MAX_INF);
            depth -= 1;
        } else {
            return score;
        }

        window = (window as f32 * ctx.params.aspw_widening_factor) as i32;
    }
}
#[inline(always)]
fn check_time(ctx: &SearchContext) {
    if ctx.stats.nodes % 2047 == 0 {
        if (ctx.start_time.elapsed() >= ctx.time_limit) || (ctx.stats.nodes >= ctx.node_limit) {
            (*ctx.stop).store(true, Ordering::Relaxed);
        }

        /*

        let nonzero: Vec<i32> = ctx.corrhist_pawn.table.iter()
            .flatten()
            .filter(|&&x| x != 0)
            .copied()
            .collect();
        eprintln!("nonzero: {}, max_abs: {}, mean_abs: {}",
                  nonzero.len(),
                  nonzero.iter().map(|x| x.abs()).max().unwrap_or(0),
                  nonzero.iter().map(|x| x.abs()).sum::<i32>() / nonzero.len().max(1) as i32
        );

         */

    }
}

#[inline(always)]
fn minors_or_majors(pos : &Chess) -> Bitboard {
    let board = pos.board();
    (board.rooks_and_queens() | board.knights() | board.bishops()) & pos.us()
}
#[inline(always)]
fn is_mate_score(score : i32) -> bool {
    score.abs() >= MATE_SCORE-128
}

