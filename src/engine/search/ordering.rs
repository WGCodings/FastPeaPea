use chess::{Board, ChessMove, Piece};

#[derive(Clone)]
pub struct MoveOrdering {
    mvv_lva: [[i32; 6]; 6],
}

impl MoveOrdering {
    pub fn new(piece_values: &[f32; 6]) -> Self {
        let mut table = [[0; 6]; 6];

        for attacker in 0..6 {
            for victim in 0..6 {
                table[attacker][victim] =
                    (piece_values[victim] as i32 + 6)
                        - (piece_values[attacker] as i32 / 100);
            }
        }

        Self { mvv_lva: table }
    }

    #[inline(always)]
    pub fn order_moves(
        &self,
        board: &Board,
        pv_move: Option<&ChessMove>,
        moves: &mut Vec<ChessMove>,
    ) {
        // 1. PV first
        if let Some(pv) = pv_move {
            if let Some(idx) = moves.iter().position(|m| m == pv) {
                moves.swap(0, idx);
            }
        }

        // 2. Partition captures
        let (mut captures, quiets): (Vec<_>, Vec<_>) =
            moves.drain(..).partition(|mv| is_capture(board, mv));

        // 3. Order captures by MVV-LVA
        self.order_captures(board, &mut captures);

        // 4. Rebuild
        moves.extend(captures);
        moves.extend(quiets);
    }

    #[inline(always)]
    pub fn order_captures(&self, board: &Board, moves: &mut [ChessMove]) {
        moves.sort_by_key(|mv| -self.mvv_lva_score(board, mv));
    }



    #[inline(always)]
    pub fn mvv_lva_score(&self, board: &Board, mv: &ChessMove) -> i32 {
        let attacker_piece = board
            .piece_on(mv.get_source())
            .expect("attacker must exist");

        let victim_piece = board
            .piece_on(mv.get_dest())
            .unwrap_or(Piece::Pawn); // en-passant fallback

        let attacker = attacker_piece.to_index();
        let victim = victim_piece.to_index();

        self.mvv_lva[attacker][victim]
    }
}
#[inline(always)]
pub fn is_capture(board: &Board, mv: &ChessMove) -> bool {
    board.piece_on(mv.get_dest()).is_some()
        || board.en_passant() == Some(mv.get_dest())
}