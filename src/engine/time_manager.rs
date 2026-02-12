
use std::time::Duration;
use shakmaty::Position;

pub fn compute_time_limit(
    pos: &impl Position,
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
    let move_count = pos.legal_moves().len() as u32;

    // Scale between 0.7x and 1.3x
    let complexity_factor = (move_count as f32 / 30.0)
        .clamp(0.7, 1.3);

    time = time.mul_f32(complexity_factor);

    // --- Safety clamps ---
    let min = Duration::from_millis(20);
    let max = remaining * 2 / 3;

    time.clamp(min, max)
}

