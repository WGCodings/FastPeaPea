use std::cmp;
use std::time::Duration;
use shakmaty::{perft, Chess, Position};
use crate::datagen::book::{generate_genfens_openings, EpdBook};
use crate::engine::params::Params;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::state::Engine;
use crate::engine::time_manager::compute_time_limit;
use crate::engine::types::PIECE_VALUES;
use crate::engine::utility::{read_position_from_fen};
use crate::engine::search::threads::Threads;
use crate::datagen::datagen_main::run_datagen;
use crate::uci::parser::{move_to_uci, uci_to_move, UciCommand};
use crate::uci::state::UciState;
const BENCH_FENS: &str = include_str!("../../assets/bench.txt");

// ---------------------------------------------------------------------------
// UciHandler owns both uci state and engine state.
// main() creates one instance and calls run().
// ---------------------------------------------------------------------------

pub struct UciHandler {
    uci:    UciState,
    engine: Engine,
}

impl UciHandler {
    pub fn new() -> Self {
        Self {
            uci:    UciState::new(),
            engine: Engine::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Main loop
    // -----------------------------------------------------------------------

    pub fn run(&mut self) {

        // bench mode for OpenBench
        if std::env::args().nth(1).as_deref() == Some("bench") {
            self.on_bench();
            return;
        }
        if let Some(first) = std::env::args().nth(1).as_deref() {
            if first.trim_start().starts_with("genfens") {
                self.on_genfens(first);
                return;
            }
        }

        use std::io::{self, BufRead};
        let stdin = io::stdin();

        for line in stdin.lock().lines() {
            let line = line.unwrap();
            let cmd  = crate::uci::parser::parse_command(&line);

            if self.handle(cmd) == LoopControl::Break {
                break;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Handle all commands
    // -----------------------------------------------------------------------

    fn handle(&mut self, cmd: UciCommand) -> LoopControl {
        match cmd {
            UciCommand::Uci         => self.on_uci(),
            UciCommand::IsReady     => self.on_isready(),
            UciCommand::UciNewGame  => self.on_ucinewgame(),
            UciCommand::Position { fen, moves } => self.on_position(fen, moves),
            UciCommand::Go { wtime, btime, winc, binc, movetime, depth, nodes }
            => self.on_go(wtime, btime, winc, binc, movetime, depth, nodes),
            UciCommand::Stop        => self.on_stop(),
            UciCommand::SetOption { name, value } => self.on_setoption(name, value),
            UciCommand::Perft { depth } => self.on_perft(depth),
            UciCommand::Bench => self.on_bench(),
            UciCommand::GenFens { raw} => self.on_genfens(&raw),
            UciCommand::Quit        => return LoopControl::Break,
            // Non-standard tuner / datagen commands
            UciCommand::DataGen       => { run_datagen(); }
            _                         => {}
        }
        LoopControl::Continue
    }

    fn on_bench(&mut self) {
        use std::time::Instant;

        let ordering  = MoveOrdering::new(&PIECE_VALUES);

        let mut total_nodes: u64 = 0;
        let mut total_time:  u128 = 0;

        for fen in BENCH_FENS.lines().filter(|l| !l.trim().is_empty()) {
            let position = match read_position_from_fen(fen) {
                Some(p) => p,
                None    => continue,
            };

            self.engine.position = position.clone();
            self.engine.init_history();
            self.engine.tt.clear();

            let timer = Instant::now();
            let (_, _, _,stats) = Threads::search(
                &position,
                &mut self.engine,
                &ordering,
                11,
                u64::MAX,
                Some(Duration::MAX/10),
                &self.uci,
                false
            );
            total_time  += timer.elapsed().as_millis();
            total_nodes += stats.nodes;
        }

        let nps = total_nodes * 1000 / (total_time as u64).max(1);
        println!("Bench: {total_nodes} nodes {nps} nps");
    }



    fn on_uci(&self) {
        println!("id name Pea 9.1");
        println!("id author Warre G.");
        println!("option name Hash type spin default 16 min 1 max 1024");
        println!("option name Threads type spin default 1 min 1 max 128");
        println!("option name Move Overhead type spin default 10 min 0 max 1000");
        println!("option name UCI_ShowWDL type check default true");
        println!("option name NormalizeScore type check default false");
        print_spsa_options(&self.engine.params);
        println!("uciok");
    }

    fn on_isready(&self) {
        println!("readyok");
    }

    fn on_ucinewgame(&mut self) {
        self.uci.stop();
        self.uci.reset_stop();

        self.engine.position = Chess::new();
        self.engine.tt.clear();
        self.engine.repetition_stack.clear();
        self.engine.corrhist_pawn.clear();
        self.engine.corrhist_material.clear();
        self.engine.history_tables.clear();
    }

    fn on_position(&mut self, fen: Option<String>, moves: Vec<String>) {
        self.uci.reset_stop();

        self.engine.position = match fen {
            Some(f) => read_position_from_fen(&f).unwrap(),
            None    => Chess::new(),
        };
        self.engine.init_history();

        for mv in moves {
            let m = uci_to_move(&self.engine.position, &mv);
            self.engine.position.play_unchecked(m);
            self.engine.push_history();
        }
    }

    fn on_go(
        &mut self,
        wtime: Option<u64>, btime: Option<u64>,
        winc:  Option<u64>, binc:  Option<u64>,
        movetime: Option<u64>, depth: Option<u32>,
        nodes: Option<u64>,
    ) {
        self.uci.save_time(wtime, btime, winc, binc);
        self.uci.reset_stop();

        let max_depth = depth.map_or(64, |d| d as usize);
        let max_nodes = nodes.unwrap_or(u64::MAX);
        let ordering  = MoveOrdering::new(&PIECE_VALUES);
        let position  = self.engine.position.clone();

        let time_limit = compute_time_limit(
            &self.engine.position,
            wtime, btime, winc, binc,
            movetime, depth,
            self.engine.options.move_overhead,
        );

        let (_, best_move, _,_) = Threads::search(
            &position, &mut self.engine, &ordering,
            max_depth, max_nodes, time_limit, &self.uci,true
        );

        println!("bestmove {}", move_to_uci(&best_move));
    }

    fn on_stop(&mut self) {
        self.uci.stop();
    }

    fn on_setoption(&mut self, name: String, value: String) {
        let n = name.as_str();
        let v = value.as_str();

        if n.eq_ignore_ascii_case("multipv") {
            if let Ok(x) = v.parse::<usize>() {
                self.uci.multipv = cmp::min(cmp::max(x, 1), 5);
            }
        } else if n.eq_ignore_ascii_case("hash") {
            if let Ok(mb) = v.parse::<usize>() {
                self.engine.resize_tt(cmp::min(cmp::max(mb, 1), 1024));
            }
        } else if n.eq_ignore_ascii_case("move overhead") {
            if let Ok(ms) = v.parse::<u64>() {
                self.engine.set_move_overhead(cmp::min(cmp::max(ms, 1), 1000));
            }
        } else if n.eq_ignore_ascii_case("threads") {
            if let Ok(t) = v.parse::<u8>() {
                self.engine.set_threads(cmp::min(cmp::max(t, 1), 128));
            }
        } else if n.eq_ignore_ascii_case("normalizescore") {
            self.uci.normalize_score = v.eq_ignore_ascii_case("true");
        } else if n.eq_ignore_ascii_case("uci_showwdl") {
            self.uci.uci_show_wdl = v.eq_ignore_ascii_case("true");
        } else {
            // Set tuning parameters
            self.set_param(n, v);
        }
    }

    fn on_perft(&self, depth: u32) {
        let start   = std::time::Instant::now();
        let nodes   = perft(&self.engine.position, depth);
        let elapsed = start.elapsed().as_millis();
        let nps     = (1000 * nodes as u128 / elapsed.max(1)) as u64;

        println!("nodes {}", nodes);
        println!("time {}", elapsed);
        println!("nps {}", nps);
        println!("perftok");
    }

    fn on_genfens(&self, raw: &str) {
        let tokens: Vec<&str> = raw.split_whitespace().collect();

        let n: usize = tokens.get(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let seed: u64 = tokens.iter()
            .position(|&t| t == "seed")
            .and_then(|i| tokens.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let book_arg: &str = tokens.iter()
            .position(|&t| t == "book")
            .and_then(|i| tokens.get(i + 1))
            .copied()
            .unwrap_or("None");

        let book = if book_arg != "None" {
            Some(EpdBook::load("assets/book.epd"))
        } else {
            None
        };

        let fixed_plies: Option<u32> = tokens.iter()
            .position(|&t| t == "plies")
            .and_then(|i| tokens.get(i + 1))
            .and_then(|s| s.parse().ok());

        let fens = generate_genfens_openings(n, seed, book.as_ref(), fixed_plies);
        for fen in fens {
            println!("info string genfens {fen}");
        }
    }

    fn set_param(&mut self, name: &str, value: &str) {
        let params = &mut self.engine.params;

        match name.to_lowercase().as_str() {
            // ints
            "raz_max_depth"          => { if let Ok(x) = value.parse::<i32>() { params.raz_max_depth = x; } }
            "raz_thr"                => { if let Ok(x) = value.parse::<i32>() { params.raz_thr = x; } }
            "raz_improving_margin"   => { if let Ok(x) = value.parse::<i32>() { params.raz_improving_margin = x; } }
            "nmp_margin"             => { if let Ok(x) = value.parse::<i32>() { params.nmp_margin = x; } }
            "nmp_scaling"            => { if let Ok(x) = value.parse::<i32>() { params.nmp_scaling = x; } }
            "nmp_improving_scaling"  => { if let Ok(x) = value.parse::<i32>() { params.nmp_improving_scaling = x; } }
            "nmp_min_depth"          => { if let Ok(x) = value.parse::<i32>() { params.nmp_min_depth = x; } }
            "nmp_base_reduction"     => { if let Ok(x) = value.parse::<i32>() { params.nmp_base_reduction = x; } }
            "nmp_reduction_scaling"  => { if let Ok(x) = value.parse::<i32>() { params.nmp_reduction_scaling = x; } }
            "nmp_verif_depth"        => { if let Ok(x) = value.parse::<i32>() { params.nmp_verif_depth = x; } }
            "snmp_scaling"           => { if let Ok(x) = value.parse::<i32>() { params.snmp_scaling = x; } }
            "lmr_min_searches"       => { if let Ok(x) = value.parse::<i32>() { params.lmr_min_searches = x; } }
            "lmr_min_depth"          => { if let Ok(x) = value.parse::<i32>() { params.lmr_min_depth = x; } }
            "lmr_history_divisor"    => { if let Ok(x) = value.parse::<i32>() { params.lmr_history_divisor = x; } }
            "lmr_see_thr"            => { if let Ok(x) = value.parse::<i32>() { params.lmr_see_thr = x; } }
            "lmr_corr_scaling"       => { if let Ok(x) = value.parse::<i32>() { params.lmr_corr_scaling = x; } }
            "aspw_min_depth"         => { if let Ok(x) = value.parse::<i32>() { params.aspw_min_depth = x; } }
            "aspw_window_size"       => { if let Ok(x) = value.parse::<i32>() { params.aspw_window_size = x; } }
            "fp_base"                => { if let Ok(x) = value.parse::<i32>() { params.fp_base = x; } }
            "fp_scaling"             => { if let Ok(x) = value.parse::<i32>() { params.fp_scaling = x; } }
            "fp_max_depth"           => { if let Ok(x) = value.parse::<i32>() { params.fp_max_depth = x; } }
            "fp_improving_margin"    => { if let Ok(x) = value.parse::<i32>() { params.fp_improving_margin = x; } }
            "fp_min_moves_searched"  => { if let Ok(x) = value.parse::<i32>() { params.fp_min_moves_searched = x; } }
            "rfp_scaling"            => { if let Ok(x) = value.parse::<i32>() { params.rfp_scaling = x; } }
            "rfp_improving_scaling"  => { if let Ok(x) = value.parse::<i32>() { params.rfp_improving_scaling = x; } }
            "rfp_max_depth"          => { if let Ok(x) = value.parse::<i32>() { params.rfp_max_depth = x; } }
            "lmp_base"               => { if let Ok(x) = value.parse::<i32>() { params.lmp_base = x; } }
            "lmp_lin_scaling"        => { if let Ok(x) = value.parse::<i32>() { params.lmp_lin_scaling = x; } }
            "lmp_quad_scaling"       => { if let Ok(x) = value.parse::<i32>() { params.lmp_quad_scaling = x; } }
            "lmp_max_depth"          => { if let Ok(x) = value.parse::<i32>() { params.lmp_max_depth = x; } }
            "cont_hist_scaling"      => { if let Ok(x) = value.parse::<i32>() { params.cont_hist_scaling = x; } }
            "cont_hist_base"         => { if let Ok(x) = value.parse::<i32>() { params.cont_hist_base = x; } }
            "cont_hist_malus_scaling"=> { if let Ok(x) = value.parse::<i32>() { params.cont_hist_malus_scaling = x; } }
            "cont_hist_malus_base"   => { if let Ok(x) = value.parse::<i32>() { params.cont_hist_malus_base = x; }}
            "hpp_quiet_scaling"      => { if let Ok(x) = value.parse::<i32>() { params.hpp_quiet_scaling = x; } }
            "hpp_tactical_scaling"   => { if let Ok(x) = value.parse::<i32>() { params.hpp_tactical_scaling = x; } }
            "iir_min_depth"          => { if let Ok(x) = value.parse::<i32>() { params.iir_min_depth = x; } }
            "se_dext_margin"         => { if let Ok(x) = value.parse::<i32>() { params.se_dext_margin = x; } }
            "se_scaling"             => { if let Ok(x) = value.parse::<i32>() { params.se_scaling = x; } }
            "se_depth_ok"            => { if let Ok(x) = value.parse::<i32>() { params.se_depth_ok = x; } }
            "se_min_depth"           => { if let Ok(x) = value.parse::<i32>() { params.se_min_depth = x; } }
            "se_text_margin"         => { if let Ok(x) = value.parse::<i32>() { params.se_text_margin = x; } }
            "se_max_nr_dext"         => { if let Ok(x) = value.parse::<i32>() { params.se_max_nr_dext = x; } }
            "hist_prune_margin"      => { if let Ok(x) = value.parse::<i32>() { params.hist_prune_margin = x; } }
            "hist_prune_depth"       => { if let Ok(x) = value.parse::<i32>() { params.hist_prune_depth = x; } }
            "pc_beta_margin"         => { if let Ok(x) = value.parse::<i32>() { params.pc_beta_margin = x; } }
            "pc_depth_divisor"       => { if let Ok(x) = value.parse::<i32>() { params.pc_depth_divisor = x; } }
            "pc_min_depth"           => { if let Ok(x) = value.parse::<i32>() { params.pc_min_depth = x; } }
            "pc_improving_margin"    => { if let Ok(x) = value.parse::<i32>() { params.pc_improving_margin = x; } }
            "pc_see_thr"             => { if let Ok(x) = value.parse::<i32>() { params.pc_see_thr = x; } }

            // floats
            "lmr_red_constant"       => { if let Ok(x) = value.parse::<i32>() { params.lmr_red_constant = x as f32 / 10000.0; } }
            "lmr_red_scaling"        => { if let Ok(x) = value.parse::<i32>() { params.lmr_red_scaling = x as f32 / 10000.0; } }
            "aspw_widening_factor"   => { if let Ok(x) = value.parse::<i32>() { params.aspw_widening_factor = x as f32 / 10000.0; } }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------

#[derive(PartialEq)]
enum LoopControl { Continue, Break }

fn print_spsa_options(params: &Params) {
    //int
    println!("option name raz_max_depth type spin default {} min 0 max 15", params.raz_max_depth );
    println!("option name raz_thr type spin default {} min 0 max 512", params.raz_thr);
    println!("option name raz_improving_margin type spin default {} min -100 max 100", params.raz_improving_margin);
    println!("option name nmp_margin type spin default {} min 0 max 250", params.nmp_margin);
    println!("option name nmp_scaling type spin default {} min 0 max 100", params.nmp_scaling);
    println!("option name nmp_improving_scaling type spin default {} min -100 max 200", params.nmp_improving_scaling);
    println!("option name nmp_min_depth type spin default {} min 2 max 10", params.nmp_min_depth);
    println!("option name nmp_base_reduction type spin default {} min 1 max 5", params.nmp_base_reduction);
    println!("option name nmp_reduction_scaling type spin default {} min 1 max 10", params.nmp_reduction_scaling);
    println!("option name nmp_verif_depth type spin default {} min 8 max 20", params.nmp_verif_depth);
    println!("option name snmp_scaling type spin default {} min 0 max 200", params.snmp_scaling);
    println!("option name lmr_min_searches type spin default {} min 1 max 10", params.lmr_min_searches);
    println!("option name lmr_min_depth type spin default {} min 1 max 10", params.lmr_min_depth);
    println!("option name lmr_history_divisor type spin default {} min 4096 max 16384", params.lmr_history_divisor);
    println!("option name lmr_see_thr type spin default {} min -10 max 100", params.lmr_see_thr);
    println!("option name lmr_corr_scaling type spin default {} min 20 max 60", params.lmr_corr_scaling);
    println!("option name aspw_min_depth type spin default {} min 1 max 10", params.aspw_min_depth);
    println!("option name aspw_window_size type spin default {} min 5 max 150", params.aspw_window_size);
    println!("option name fp_base type spin default {} min 0 max 120", params.fp_base);
    println!("option name fp_scaling type spin default {} min 0 max 120", params.fp_scaling);
    println!("option name fp_max_depth type spin default {} min 0 max 15", params.fp_max_depth);
    println!("option name fp_improving_margin type spin default {} min 0 max 200", params.fp_improving_margin);
    println!("option name fp_min_moves_searched type spin default {} min 1 max 10", params.fp_min_moves_searched);
    println!("option name rfp_scaling type spin default {} min 0 max 150", params.rfp_scaling);
    println!("option name rfp_improving_scaling type spin default {} min 0 max 200", params.rfp_improving_scaling);
    println!("option name rfp_max_depth type spin default {} min 0 max 25", params.rfp_max_depth);
    println!("option name lmp_base type spin default {} min 0 max 10", params.lmp_base);
    println!("option name lmp_lin_scaling type spin default {} min 0 max 10", params.lmp_lin_scaling);
    println!("option name lmp_quad_scaling type spin default {} min 0 max 10", params.lmp_quad_scaling);
    println!("option name lmp_max_depth type spin default {} min 0 max 15", params.lmp_max_depth);
    println!("option name cont_hist_scaling type spin default {} min 200 max 1000", params.cont_hist_scaling);
    println!("option name cont_hist_base type spin default {} min 100 max 300", params.cont_hist_base);
    println!("option name cont_hist_malus_scaling type spin default {} min 200 max 1000", params.cont_hist_malus_scaling);
    println!("option name cont_hist_malus_base type spin default {} min 100 max 300", params.cont_hist_malus_base);
    println!("option name hpp_tactictal_scaling type spin default {} min 20 max 160", params.hpp_tactical_scaling);
    println!("option name hpp_quiet_scaling type spin default {} min 0 max 100", params.hpp_quiet_scaling);
    println!("option name iir_min_depth type spin default {} min 0 max 10", params.iir_min_depth);
    println!("option name se_dext_margin type spin default {} min 0 max 100", params.se_dext_margin);
    println!("option name se_scaling type spin default {} min 0 max 10", params.se_scaling);
    println!("option name se_depth_ok type spin default {} min 1 max 10", params.se_depth_ok);
    println!("option name se_min_depth type spin default {} min 2 max 12", params.se_min_depth);
    println!("option name se_text_margin type spin default {} min 25 max 150", params.se_text_margin);
    println!("option name se_max_nr_dext type spin default {} min 2 max 16", params.se_max_nr_dext);
    println!("option name hist_prune_margin type spin default {} min 512 max 2048", params.hist_prune_margin);
    println!("option name hist_prune_depth type spin default {} min 2 max 10", params.hist_prune_depth);
    println!("option name pc_beta_margin type spin default {} min 64 max 512", params.pc_beta_margin);
    println!("option name pc_depth_divisor type spin default {} min 1 max 300", params.pc_depth_divisor);
    println!("option name pc_min_depth type spin default {} min 3 max 10", params.pc_min_depth);
    println!("option name pc_improving_margin type spin default {} min -100 max 100", params.pc_improving_margin);
    println!("option name pc_see_thr type spin default {} min -256 max 256", params.pc_see_thr);

    // float
    println!("option name lmr_red_constant type spin default {} min 5000 max 25000", (params.lmr_red_constant * 10000.0) as i32);
    println!("option name lmr_red_scaling type spin default {} min 10000 max 50000", (params.lmr_red_scaling * 10000.0) as i32);
    println!("option name aspw_widening_factor type spin default {} min 12000 max 50000", (params.aspw_widening_factor * 10000.0) as i32);
}