use shakmaty::{Chess, Color, Position, Role};
use shakmaty::attacks::{bishop_attacks, knight_attacks, rook_attacks};
use crate::engine::params::Params;



#[inline(always)]
pub fn evaluate(pos: &Chess, params: &Params) -> f32 {
    let board = pos.board();
    let mut score = 0.0;

    for &role in &[
        Role::Pawn,
        Role::Knight,
        Role::Bishop,
        Role::Rook,
        Role::Queen,
        Role::King,
    ] {
        let idx = role as usize - 1;
        let val = params.piece_values[idx] * params.material_weight;

        let white = board.by_color(Color::White) & board.by_role(role);
        let black = board.by_color(Color::Black) & board.by_role(role);

        score += val * white.count() as f32;
        score -= val * black.count() as f32;
    }
    // === MOBILITY ===
    let white_mob = mobility_score(pos, params, Color::White);
    let black_mob = mobility_score(pos, params,Color::Black);
    score += (white_mob - black_mob) as f32 ;
    score +=  add_tempo_bonus(pos, params);

    if pos.turn() == Color::White {
        score
    } else {
        -score
    }
}
#[inline(always)]
fn add_tempo_bonus(pos: &Chess,params: &Params) -> f32{
    if pos.turn() == Color::White {
        params.tempo_bonus
    } else {
        -params.tempo_bonus
    }
}
#[inline(always)]
fn mobility_score(pos: &Chess, params: &Params,color: Color) -> i32 {
    let board = pos.board();
    let occ = board.occupied();
    let own = board.by_color(color);

    let mut score = 0;

    // === KNIGHTS ===
    let knights = board.by_role(Role::Knight) & own;
    for sq in knights {
        let attacks = knight_attacks(sq) & !own;
        score += attacks.count() as i32 * params.mobility_bonus[Role::Knight as usize-1];
    }

    // === BISHOPS ===
    let bishops = board.by_role(Role::Bishop) & own;
    for sq in bishops {
        let attacks = bishop_attacks(sq,occ) & !own;
        score += attacks.count() as i32 * params.mobility_bonus[Role::Bishop as usize-1];
    }
    // === ROOKS ===
    let rooks = board.by_role(Role::Rook) & own;
    for sq in rooks {
        let attacks =
            rook_attacks(sq,occ) & !own;
        score += attacks.count() as i32 * params.mobility_bonus[Role::Rook as usize-1];
    }

    // === QUEENS ===
    let queens = board.by_role(Role::Queen) & own;
    for sq in queens {
        let attacks =
            (bishop_attacks(sq,occ) | rook_attacks(sq,occ)) & !own;
        score += attacks.count() as i32 * params.mobility_bonus[Role::Queen as usize-1];
    }

    score
}





