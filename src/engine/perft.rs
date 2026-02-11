use chess::{Board, MoveGen};

pub fn perft(board: &Board, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = MoveGen::new_legal(board);

    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0;

    for mv in moves {
        let child = board.make_move_new(mv);
        nodes += perft(&child, depth - 1);
    }

    nodes
}
