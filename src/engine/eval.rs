use std::cmp::max;
use shakmaty::{Bitboard, Chess, Color, Position, Role, Square};
use crate::engine::search::context::{clean_accumulator, NNUEState};
use crate::engine::types::{KBN_TABLE_DARK, KBN_TABLE_LIGHT, MATE_SCORE};
use crate::nnue::network::{accumulator_for_perspective, Network};

// =====================================================================================================================//
// EVALUATE NNUE + MOPUP                                                                                                //
// =====================================================================================================================//
pub fn evaluate(pos: &Chess, net: &Network, state: &mut NNUEState) -> i32 {

    clean_accumulator(net, state);

    let (us, them) = match pos.turn() {
        Color::White => (&state.white_acc, &state.black_acc),
        Color::Black => (&state.black_acc, &state.white_acc),
    };

    let nnue_score= net.evaluate(us,them,pos);

    let mopup_score = mopup_evaluation(pos,nnue_score);

    (nnue_score+mopup_score).clamp(-MATE_SCORE + 1000, MATE_SCORE - 1000)
}

// =====================================================================================================================//
// MOPUP EVAL TO HELP MATEFINDING KRK KQK KBNK , MIGHT BE OBSOLETE NOW WITH BETTER SEARCH                               //
// =====================================================================================================================//
// TODO TEST IS MOPUP IS STILL NEEDED NOW I CAN GET TO DEPTH 20+ EASILY
#[inline(always)]
fn mopup_evaluation(pos: &Chess,score : i32) -> i32{
    let board = pos.board();
    let mut mop_bonus = 0;

    // ---------------------- MOPUP EVALUATION --------------------------------------------- //
    let mopup_condition =
        board.occupied().count() == 3 && (board.rooks().count()==1 || board.queens().count()==1)
            || (board.occupied().count() == 4 && board.bishops().count()==2);
    let knbk_condition = board.occupied().count() == 4 && board.bishops().count()==1 && board.knights().count()==1;

    let color_value = if score < 0 {-1} else {1};
    let on_turn = pos.turn();
    if mopup_condition  && score.abs() > 1000 {

        let winning_king : Square =  if score > 0 {board.king_of(on_turn).unwrap()} else {board.king_of(!on_turn).unwrap()};
        let losing_king : Square   = if score < 0 {board.king_of(on_turn).unwrap()} else {board.king_of(!on_turn).unwrap()};

        let dst_between_kings =
            (winning_king.file()-losing_king.file()).abs()
                + (winning_king.rank()-losing_king.rank()).abs();
        let opponent_king_dst_from_centre : i32 =
            max(3-losing_king.rank() as i32, losing_king.rank() as i32 -4)
                + max(3-losing_king.file() as i32, losing_king.file() as i32 -4);


        mop_bonus = ((14 - dst_between_kings) * 4 + opponent_king_dst_from_centre * 10) * 15;
    }
    else {
        if knbk_condition  && score.abs() > 500{

            let winning_king : Square =  if score > 0 {board.king_of(on_turn).unwrap()} else {board.king_of(!on_turn).unwrap()};
            let losing_king : Square   = if score < 0 {board.king_of(on_turn).unwrap()} else {board.king_of(!on_turn).unwrap()};

            let bishop = board.occupied() & board.bishops();
            let bishop_bb : Bitboard = Bitboard(bishop.0 & bishop.0.wrapping_neg());
            let bishop_square: Square = Square::new(bishop_bb.0.trailing_zeros());
            let is_bishop_on_dark = bishop_square.is_dark();

            let idx = u32::from(losing_king) as usize;


            let dst_between_kings = max((winning_king.file()-losing_king.file()).abs(),(winning_king.rank()-losing_king.rank()).abs());
            mop_bonus = 0x780 - (dst_between_kings *16);

            if is_bishop_on_dark {
                mop_bonus -= KBN_TABLE_DARK[idx]*4;
            } else {
                mop_bonus -= KBN_TABLE_LIGHT[idx]*4;
            }
        }
    }
    mop_bonus *color_value
}

// This is the hce i used initially LOL
fn _hce(pos: &Chess) -> i32 {
    let piece_values = [100,300,320,500,900,10000];
    let board = pos.board();
    let mut score = 0;
    for &role in &[
        Role::Pawn,
        Role::Knight,
        Role::Bishop,
        Role::Rook,
        Role::Queen,
        Role::King,
    ] {
        let idx = role as usize - 1;
        let val = piece_values[idx];
        let white = board.by_color(Color::White) & board.by_role(role);
        let black = board.by_color(Color::Black) & board.by_role(role);
        score += val * white.count() as i32;
        score -= val * black.count() as i32;
    }
    let mobility = pos.legal_moves().len() as i32;
    let mobility_bonus = if pos.turn() == Color::White { mobility  } else { -mobility };

    score + mobility_bonus
}
