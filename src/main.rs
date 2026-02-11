mod engine;
mod uci;

use std::io::{self, BufRead};
use std::sync::atomic::Ordering;
use std::time::Duration;
use shakmaty::{perft, Chess, Color, Position};
use crate::uci::{parser::*, state::*};
use crate::engine::search::search::{search};
use crate::engine::params::Params;
use crate::engine::time_manager::compute_time_limit;
use crate::engine::utility::read_position_from_fen;

fn main() {
    let debug = false; // change to false for UCI

    if debug {
        let fen = "2k4r/1r1q2pp/QBp2p2/1p6/8/8/P4PPP/2R3K1 w - - 1 0";
        let pos = read_position_from_fen(fen).unwrap();


        let params = Params::default();
        let max_depth = 50;
        let time_remaining = Duration::from_millis(10000);
        let multipv =3;


        let (best_move, score, stats,multipv_lines) =
            search(&pos, &params, max_depth, multipv, Some(time_remaining));

        println!("Best move: {:?}", move_to_uci(&best_move));
        println!("Score: {:.2}", score);
        println!("Time taken: {:?}", stats.duration);
        println!("Nodes searched: {}", stats.nodes);
        println!("NPS: {:.0}", stats.nodes as f64/stats.duration.as_secs_f64());
        println!("Seldepth: {}", stats.seldepth);
        println!("Average depth: {:.2}", stats.depth_sum/stats.depth_samples);
        println!("\nMultiPV:");
        for (i, (score, line)) in multipv_lines.lines.iter().enumerate() {
            print!("{}: score {:.2} pv", i + 1, score);
            for mv in line {
                print!(" {}", move_to_uci(mv));
            }
            println!();
        }
    } else {
        // UCI loop
        let stdin = io::stdin();
        let mut state = UciState::new();
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
                UciCommand::UciNewGame => state.position = Chess::default(),
                UciCommand::Position { fen, moves } => {
                    if let Some(fen) = fen {
                        state.position = read_position_from_fen(&fen).unwrap();
                    } else {
                        state.position = Chess::default();
                    }
                    for mv in moves {
                        let m = uci_to_move(&state.position, &mv);
                        state.position.play_unchecked(&m);
                    }
                }
                UciCommand::Go { wtime, btime, winc, binc, movetime,depth } => {
                    state.stop.store(false, Ordering::Relaxed);

                    let max_depth = if let Some(d) = depth {
                        d as usize
                    } else {
                        64
                    };


                    let remaining = match state.position.turn() {
                        Color::White => wtime.map(Duration::from_millis),
                        Color::Black => btime.map(Duration::from_millis),
                    };

                    let increment = match state.position.turn() {
                        Color::White => winc.map(Duration::from_millis),
                        Color::Black => binc.map(Duration::from_millis),
                    };

                    let time_limit = if let Some(ms) = movetime {
                        Some(Duration::from_millis(ms))
                    } else {
                        if max_depth !=64 {
                            Some(Duration::MAX/10)
                        }
                        else { Some(compute_time_limit(&state.position, remaining, increment)) }
                    };

                    let (best_move, _score, stats,multipv_lines) = search(
                        &state.position,
                        &params,
                        max_depth, // max depth
                        state.multipv,
                        time_limit,
                    );

                    let elapsed = stats.duration.as_secs_f64();
                    let elapsed_millis = stats.duration.as_millis();
                    let nps = if elapsed > 0.0 {
                        (stats.nodes as f64 / elapsed) as u64
                    } else {
                        0
                    };

                    for (i, (score, line)) in multipv_lines.lines.iter().enumerate() {
                        let pv_string = pv_to_string(line);
                        println!(
                            "info depth {:.0} seldepth {} multipv {} score cp {} nodes {} nps {} time {} pv {}",
                            line.len(),
                            stats.seldepth,
                            i+1,
                            score,
                            stats.nodes,
                            nps,
                            elapsed_millis,
                            pv_string
                        );
                    }
                    println!("bestmove {}", move_to_uci(&best_move));
                }
                UciCommand::SetOption { name, value } => {
                    if name.eq_ignore_ascii_case("multipv") {
                        if let Ok(n) = value.parse::<usize>() {
                            println!("{}", n);
                            state.multipv = n.max(1).min(5);
                        }
                    }
                }
                UciCommand::Stop => state.stop.store(true, Ordering::Relaxed),
                UciCommand::Perft { depth } => {
                    let start = std::time::Instant::now();
                    let nodes = perft(&state.position, depth);
                    let elapsed = start.elapsed().as_millis();
                    let nps = (nodes as u128 / elapsed) as u64;

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
        s.push_str(&move_to_uci(mv));
    }
    s
}
