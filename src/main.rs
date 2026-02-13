mod engine;
mod uci;

use std::cmp;
use std::io::{self, BufRead};
use std::sync::atomic::Ordering;
use std::time::Duration;

use shakmaty::{perft, Chess, Color, Position};
use crate::uci::{parser::*, state::*};
use crate::engine::search::search::{search, SearchStats};
use crate::engine::params::Params;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::search::context::*;
use crate::engine::search::pv::{MultiPv, PvTable};
use crate::engine::time_manager::compute_time_limit;
use crate::engine::state::*;
use crate::engine::utility::read_position_from_fen;

fn main() {
    let debug = false;

    if debug {
        let fen = "2k4r/1r1q2pp/QBp2p2/1p6/8/8/P4PPP/2R3K1 w - - 1 0";
        let pos = read_position_from_fen(fen).unwrap();

        let params = Params::default();
        let max_depth = 50;
        let time_remaining = Duration::from_millis(10000);
        let multipv = 3;

        let ordering = MoveOrdering::new(&params.piece_values);

        let engine_state = EngineState::new(128);
        let mut tt = engine_state.tt;

        let mut ctx = SearchContext::new(&params, &ordering,multipv,&mut tt);

        let score = search(&pos, &mut ctx, max_depth, Some(time_remaining));

        let best_move = ctx.pv.best_move().unwrap();
        let stats = ctx.stats;
        let multipv_lines = ctx.multipv;
        let tt_occupancy = ctx.tt.tt_occupancy();


        println!("Best move: {:?}", move_to_uci(&best_move));
        println!("Score: {:.2}", score);
        println!("Time taken: {:?}", stats.duration);
        println!("Nodes searched: {}", stats.nodes);
        println!("NPS: {:.0}", stats.nodes as f64 / stats.duration.as_secs_f64());
        println!("Seldepth: {}", stats.seldepth);
        println!("Average depth: {:.2}", stats.depth_sum / stats.depth_samples);
        println!("TT occupancy: {:.0}", tt_occupancy);
        println!("\nMultiPV:");
        let multi_pv_lines = &multipv_lines.lines;
        for (i, (score, line)) in multi_pv_lines.iter().enumerate() {
            print!("{}: score {:.2} pv", i + 1, score);
            for mv in line {
                print!(" {}", move_to_uci(mv));
            }
            println!();
        }
    } else {
        let stdin = io::stdin();
        let mut uci_state = UciState::new();
        let mut engine_state = EngineState::new(128);
        let params = Params::default();

        for line in stdin.lock().lines() {
            let line = line.unwrap();
            let cmd = parse_command(&line);

            match cmd {
                UciCommand::Uci => {
                    println!("id name FastPeaPea");
                    println!("id author Warre G.");
                    println!("option name MultiPV type spin default 1 min 1 max 5");
                    println!("uciok");
                }

                UciCommand::IsReady => println!("readyok"),

                UciCommand::UciNewGame => {
                    uci_state.position = Chess::new();
                    engine_state.position = Chess::new();
                    engine_state.tt.clear();
                    engine_state.repetition_stack.clear();
                }

                UciCommand::Position { fen, moves } => {
                    if let Some(fen) = fen {
                        engine_state.position = read_position_from_fen(&fen).unwrap();
                    } else {
                        engine_state.position = Chess::new();
                    }
                    engine_state.init_history();

                    for mv in moves {
                        let m = uci_to_move(&engine_state.position, &mv);
                        engine_state.position.play_unchecked(m);
                        engine_state.increase_history();
                    }
                }

                UciCommand::Go {
                    wtime,
                    btime,
                    winc,
                    binc,
                    movetime,
                    depth,
                } => {
                    uci_state.stop.store(false, Ordering::Relaxed);

                    let max_depth = if let Some(d) = depth {
                        d as usize
                    } else {
                        64
                    };

                    let remaining = match engine_state.position.turn() {
                        Color::White => wtime.map(Duration::from_millis),
                        Color::Black => btime.map(Duration::from_millis),
                    };

                    let increment = match engine_state.position.turn() {
                        Color::White => winc.map(Duration::from_millis),
                        Color::Black => binc.map(Duration::from_millis),
                    };

                    let time_limit = if let Some(ms) = movetime {
                        Some(Duration::from_millis(ms))
                    } else {
                        if max_depth != 64 {
                            Some(Duration::MAX / 10)
                        } else {
                            Some(compute_time_limit(
                                &engine_state.position,
                                remaining,
                                increment,
                            ))
                        }
                    };

                    let ordering = MoveOrdering::new(&params.piece_values);
                    let repetition_stack = &engine_state.repetition_stack;

                    let mut ctx = SearchContext {
                        params: &params,
                        ordering: &ordering,
                        pv: PvTable::new(64),
                        stats: SearchStats::default(),
                        multipv: MultiPv::new(uci_state.multipv),
                        repetition_stack: repetition_stack.to_vec(),
                        tt: &mut engine_state.tt,
                    };

                    let _score = search(
                        &engine_state.position,
                        &mut ctx,
                        max_depth,
                        time_limit,
                    );

                    let stats = ctx.stats;
                    let multipv_lines = ctx.multipv;
                    let best_move = ctx.pv.best_move().unwrap();
                    let tt_occupancy = ctx.tt.tt_occupancy();


                    let elapsed = stats.duration.as_secs_f64();
                    let elapsed_millis = stats.duration.as_millis();

                    let nps = if elapsed > 0.0 {
                        (stats.nodes as f64 / elapsed) as u64
                    } else {
                        0
                    };
                    let multi_pv_lines = &multipv_lines.lines;
                    for (i, (score, line)) in
                        multi_pv_lines.iter().enumerate()
                    {
                        let pv_string = pv_to_string(line);

                        println!(
                            "info depth {:.0} seldepth {} multipv {} score cp {} nodes {} nps {} hashfull {} time {} pv {}",
                            line.len(),
                            stats.seldepth,
                            i + 1,
                            score,
                            stats.nodes,
                            nps,
                            tt_occupancy,
                            elapsed_millis,
                            pv_string
                        );
                    }

                    println!("bestmove {}", move_to_uci(&best_move));
                }

                UciCommand::SetOption { name, value } => {
                    if name.as_str().eq_ignore_ascii_case("multipv") {
                        if let Ok(n) = value.as_str().parse::<usize>() {
                            println!("{}", n);
                            uci_state.multipv = cmp::min(cmp::max(n,1),5);
                        }
                    }
                }

                UciCommand::Stop => {
                    uci_state.stop.store(true, Ordering::Relaxed);
                }

                UciCommand::Perft { depth } => {
                    let start = std::time::Instant::now();
                    let nodes = perft(&engine_state.position, depth);
                    let elapsed = start.elapsed().as_millis();
                    let nps = (1000*nodes as u128 / elapsed) as u64;

                    println!("nodes {}", nodes);
                    println!("time {:.3}", elapsed);
                    println!("nps {}", nps);
                    println!("perftok");
                }

                UciCommand::Quit => break,

                _ => {}
            }
        }
    }
}

fn pv_to_string(line: &[shakmaty::Move]) -> String {
    let mut s = String::new();

    for mv in line {
        s.push(' ');
        s.push_str(move_to_uci(mv).as_str());
    }

    s
}
