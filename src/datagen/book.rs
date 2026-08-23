use std::fs;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;
use shakmaty::{Chess, EnPassantMode, Position};
use shakmaty::fen::Fen;
use crate::engine::utility::read_position_from_fen;

const MIN_RANDOM_PLIES: u32 = 6;
const MAX_RANDOM_PLIES: u32 = 9;
const FALLBACK_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

pub struct EpdBook {
    fens: Vec<String>,
}

impl EpdBook {
    pub fn load(path: &str) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;

        let fens: Vec<String> = content
            .lines()
            .filter_map(|l| {
                let line = l.trim();
                if line.is_empty() {
                    return None;
                }

                // Split into whitespace parts
                let parts: Vec<&str> = line.split_whitespace().collect();

                if parts.len() < 4 {
                    return None; // not a valid FEN/EPD line
                }

                // Take only the first 4 fields (valid FEN core)
                let fen = format!(
                    "{} {} {} {}",
                    parts[0], parts[1], parts[2], parts[3]
                );

                Some(fen)
            })
            .collect();

        Some(Self { fens })
    }

    /// Pick a random position from the book.
    pub fn random_position(&self, rng: &mut impl rand::Rng) -> Option<Chess> {
        if self.fens.is_empty() {
            return None;
        }

        let fen = &self.fens[rng.random_range(0..self.fens.len())];
        read_position_from_fen(fen)
    }
}

pub fn generate_genfens_openings(
    n: usize,
    seed: u64,
    book: Option<&EpdBook>,
    fixed_plies: Option<u32>,
) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n)
        .map(|_| generate_random_opening(&mut rng, book, fixed_plies))
        .collect()
}

fn generate_random_opening(rng: &mut StdRng, book: Option<&EpdBook>, fixed_plies: Option<u32>) -> String {
    for _ in 0..10 {
        let mut pos: Chess = book.and_then(|b| b.random_position(rng)).unwrap_or_else(|| Chess::new());

        let target_plies = fixed_plies.unwrap_or_else(|| {
            let span = MAX_RANDOM_PLIES - MIN_RANDOM_PLIES + 1;
            MIN_RANDOM_PLIES + rng.random_range(0..span)
        });

        let mut walk_ok = true;
        for _ in 0..target_plies {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                walk_ok = false;
                break;
            }
            let idx = rng.random_range(0..moves.len());
            let mv = moves[idx].clone();
            pos.play_unchecked(mv);
        }

        if walk_ok && !pos.legal_moves().is_empty() {
            return Fen::from_position(&pos, EnPassantMode::Legal).to_string();
        }
    }

    FALLBACK_FEN.to_string()
}