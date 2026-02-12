use shakmaty::{Chess, Position};

pub fn perft(pos: &Chess, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = pos.legal_moves();

    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0;

    for mv in moves {
        let mut child = pos.clone();
        child.play_unchecked(&mv);
        nodes += perft(&child, depth - 1);
    }

    nodes
}
