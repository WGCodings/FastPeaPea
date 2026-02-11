use chess::{Board, Color, Piece, get_knight_moves, get_rook_moves, get_bishop_moves};

use crate::engine::params::Params;


#[inline(always)]
pub fn evaluate(board: &Board, params: &Params) -> f32 {
    let mut score = 0.0;

    for piece in [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
        Piece::King,
    ] {
        let idx = piece.to_index();
        let val = params.piece_values[idx] * params.material_weight;

        let white = board.pieces(piece) & board.color_combined(Color::White);
        let black = board.pieces(piece) & board.color_combined(Color::Black);

        score += val * white.popcnt() as f32;
        score -= val * black.popcnt() as f32;
    }

    let white_mob = mobility_score(board, params, Color::White);
    let black_mob = mobility_score(board, params, Color::Black);

    score += (white_mob - black_mob) as f32;
    score += add_tempo_bonus(board, params);

    if board.side_to_move() == Color::White {
        score
    } else {
        -score
    }
}
#[inline(always)]
fn add_tempo_bonus(board: &Board, params: &Params) -> f32 {
    if board.side_to_move() == Color::White {
        params.tempo_bonus
    } else {
        -params.tempo_bonus
    }
}

#[inline(always)]
fn mobility_score(board: &Board, params: &Params, color: Color) -> i32 {
    let mut score = 0;

    let own = board.color_combined(color);
    let occ = board.combined();

    // === KNIGHTS ===
    let knights = board.pieces(Piece::Knight) & own;
    for sq in knights {
        let attacks = get_knight_moves(sq) & !own;
        score += attacks.popcnt() as i32
            * params.mobility_bonus[Piece::Knight.to_index()];
    }

    // === BISHOPS ===
    let bishops = board.pieces(Piece::Bishop) & own;
    for sq in bishops {
        let attacks = get_bishop_moves(sq, *occ) & !own;
        score += attacks.popcnt() as i32
            * params.mobility_bonus[Piece::Bishop.to_index()];
    }

    // === ROOKS ===
    let rooks = board.pieces(Piece::Rook) & own;
    for sq in rooks {
        let attacks = get_rook_moves(sq, *occ) & !own;
        score += attacks.popcnt() as i32
            * params.mobility_bonus[Piece::Rook.to_index()];
    }

    // === QUEENS ===
    let queens = board.pieces(Piece::Queen) & own;
    for sq in queens {
        let attacks = (get_bishop_moves(sq, *occ) |get_rook_moves(sq, *occ)) & !own;
        score += attacks.popcnt() as i32
            * params.mobility_bonus[Piece::Queen.to_index()];
    }

    score
}








