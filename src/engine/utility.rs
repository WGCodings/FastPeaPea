use chess::Board;
use std::str::FromStr;

/// Reads a FEN string and converts it to a `Board`.
pub fn read_position_from_fen(fen_str: &str) -> Option<Board> {
    Board::from_str(fen_str).ok()
}
