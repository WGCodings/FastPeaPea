use shakmaty::{Chess, fen::Fen, CastlingMode};


/// Reads a FEN string and converts it to a `Chess` position.
pub fn read_position_from_fen(fen_str: &str) -> Option<Chess> {
    let fen: Fen = fen_str.parse().ok()?; // Parse the FEN string
    fen.into_position(CastlingMode::Standard).ok() // Convert to `Chess` position
}