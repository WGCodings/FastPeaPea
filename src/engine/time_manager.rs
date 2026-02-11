use std::time::Duration;
use chess::{Board, MoveGen};

pub fn compute_time_limit(
    pos: &Board,
    remaining: Option<Duration>,
    increment: Option<Duration>,
) -> Duration {
    let remaining = match remaining {
        Some(t) => t,
        None => return Duration::from_secs(1),
    };

    let increment = increment.unwrap_or(Duration::ZERO);

    // --- Base allocation ---
    let mut time = remaining / 10 + increment;

    // --- Complexity adjustment ---
    let move_count = MoveGen::new_legal(pos).len() as u32;

    let complexity_factor = (move_count as f32 / 30.0)
        .clamp(0.7, 1.3);

    time = time.mul_f32(complexity_factor);

    let min = Duration::from_millis(20);
    let max = remaining * 2 / 3;

    time.clamp(min, max)
}

