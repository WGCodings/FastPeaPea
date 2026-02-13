use shakmaty::{Chess, Move, MoveList, Position, Role};

#[derive(Clone)]
pub struct MoveOrdering {
    mvv_lva: [[i32; 6]; 6],
}

impl MoveOrdering {
    pub fn new(piece_values: &[f32; 6]) -> Self {
        let mut table = [[0; 6]; 6];

        for attacker in 0..6 {
            for victim in 0..6 {
                // Higher = better
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
        pos: &Chess,
        pv_move: Option<&Move>,
        tt_move: Option<&Move>,
        moves: &mut MoveList,
    ) {
        // 1. PV move first
        if let Some(pv) = pv_move {
            if let Some(idx) = moves.iter().position(|m| m == pv) {
                moves.swap(0, idx);
            }
        }

        // 2. TT move second (if not same as PV)
        if let Some(tt) = tt_move {
            if Some(tt) != pv_move {
                if let Some(idx) = moves.iter().position(|m| m == tt) {
                    let insert = if pv_move.is_some() { 1 } else { 0 };
                    moves.swap(insert, idx);
                }
            }
        }

        let (mut captures, quiets): (Vec<_>, Vec<_>) =
            moves.drain(..).partition(|mv| mv.is_capture());

        self.order_captures(pos, &mut captures);

        moves.extend(captures);
        moves.extend(quiets);
    }



    #[inline(always)]
    pub fn order_captures(&self, pos: &Chess, moves: &mut [Move]) {
        moves.sort_by_key(|mv| -self.mvv_lva_score(pos, mv));
    }
    #[inline(always)]
    pub fn mvv_lva_score(&self, pos: &Chess, mv: &Move) -> i32 {
        let board = pos.board();

        let attacker_role = board
            .role_at(mv.from().unwrap())
            .expect("attacker must exist");

        let victim_role = board
            .role_at(mv.to())
            .unwrap_or(Role::Pawn); // en passant

        let attacker = attacker_role as usize - 1;
        let victim = victim_role as usize - 1;

        self.mvv_lva[attacker][victim]
    }
}
