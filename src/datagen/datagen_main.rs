// datagen_main
// Entry point for data generation, called from the UCI loop.
// Usage (in UCI): datagen [threads <n>] [nodes <n>] [target <n>]
//
// Example:
//   datagen threads 4 nodes 5000 target 1000000

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::thread;
use std::fs;
use crate::datagen::book::EpdBook;
use crate::datagen::datagen_config::DatagenConfig;
use crate::datagen::datagen_format::{RawDataWriter, thread_output_path};
use crate::datagen::game::run_game;

use crate::engine::params::Params;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::tt::TranspositionTable;
use crate::nnue::network::{Network};

/// Parse and run datagen from a UCI command string.
/// Called from your UCI loop when the command starts with "datagen".
pub fn run_datagen() {
    let config = DatagenConfig::default();
    print_config(&config);
    fs::create_dir_all(&config.output_dir).expect("Failed to create output dir");
    start(config);
}



// ------------------------------------------------------------------ //
// Main entry                                                           //
// ------------------------------------------------------------------ //

fn start(config: DatagenConfig) {

    let total_positions = Arc::new(AtomicU64::new(0));
    let start_time      = Instant::now();
    let target          = config.target_positions;

    // Spawn one worker per thread
    let handles: Vec<_> = (0..config.num_threads)
        .map(|thread_id| {
            let config          = config.clone();
            let total_positions = total_positions.clone();
            println!("Starting thread {}", thread_id);

            thread::spawn(move || { run_worker(thread_id, config, total_positions);})
        })
        .collect();

    // Progress loop on calling thread
    loop {
        thread::sleep(std::time::Duration::from_secs(5));

        let n       = (*total_positions).load(Ordering::Relaxed);
        let elapsed = start_time.elapsed().as_secs_f32();
        let pct     = n as f32 / target as f32 * 100.0;
        let pos_sec = n as f32 / elapsed;

        println!(
            "info string datagen | positions {}/{} ({:.1}%) | {:.0} pos/s | {:.0}s elapsed",
            n, target, pct, pos_sec, elapsed
        );

        if n >= target { break; }
    }

    for h in handles { h.join().ok(); }

    println!(
        "info string datagen done | {} positions in {:.1}s",
        (*total_positions).load(Ordering::Relaxed),
        start_time.elapsed().as_secs_f32(),
    );
}

// ------------------------------------------------------------------ //
// Worker                                                               //
// ------------------------------------------------------------------ //

fn run_worker(
    thread_id:      usize,
    config:         DatagenConfig,
    total_positions: Arc<AtomicU64>,
) {
    let mut rng  = rand::rng();
    let path    = thread_output_path(&config.output_dir.as_str(), thread_id);
    let mut writer = RawDataWriter::open(&path.as_str()).expect("Failed to open output file");

    let params   = Params::default();
    let ordering = MoveOrdering::new(&[100,300,300,500,900,0]);

    let mut tt_0 = TranspositionTable::new(16);
    let mut tt_1 = TranspositionTable::new(16);

    let net = Network::load();

    let mut game_id: u64 = 0;

    let book = config.epd_path.as_ref().map(|path| EpdBook::load(path)).expect("Failed to load epd");

    loop {
        if (*total_positions).load(Ordering::Relaxed) >= config.target_positions {
            break;
        }

        let (net, white_tt, black_tt) = if game_id % 2 == 0 {
            (&net, &tt_0, &tt_1)
        } else {
            (&net, &tt_1, &tt_0)
        };


        let positions = run_game(
            &config,
            book.as_ref(),
            net,
            &params, &ordering,
            white_tt, black_tt,
            &mut rng,
        );

        if !positions.is_empty() {
            writer.write_batch(&positions).expect("Write failed");
            (*total_positions).fetch_add(positions.len() as u64, Ordering::Relaxed);
        }

        if game_id % 100 == 0 {
            writer.flush().ok();
        }

        tt_0.clear();
        tt_1.clear();

        game_id += 1;
    }

    writer.flush().ok();
    println!("info string thread {:02} done | {} positions written", thread_id, writer.positions_written());
}

// ------------------------------------------------------------------ //
// Helpers                                                              //
// ------------------------------------------------------------------ //

fn print_config(config: &DatagenConfig) {
    println!("info string datagen starting");
    println!("info string   threads : {}",                          config.num_threads);
    println!("info string   nodes   : {}",                          config.nodes_per_move);
    println!("info string   target  : {} positions",                config.target_positions);
    println!("info string   adjudication plies  : {}",              config.adjudication_plies);
    println!("info string   draw adjudication score : {} ",         config.draw_adjudication_score);
    println!("info string   win/loss adjudication score : {} ",     config.adjudication_score);
    println!("info string   random opening plies : {} ",            config.random_opening_plies);
    println!("info string   output  : {}",                          config.output_dir);
}

