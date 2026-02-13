use shakmaty::{Chess, Position};


use crate::engine::params::Params;
use crate::engine::search::ordering::MoveOrdering;
use crate::engine::search::pv::{MultiPv, PvTable};
use crate::engine::search::search::SearchStats;
use crate::engine::tt::TranspositionTable;

pub struct SearchContext<'a> {
    pub params: &'a Params,
    pub ordering: &'a MoveOrdering,
    pub pv: PvTable,
    pub stats: SearchStats,
    pub multipv: MultiPv,
    pub repetition_stack: Vec<u64>,
    pub tt: &'a mut TranspositionTable,
}

impl<'a> SearchContext<'a> {
    pub fn new(
        params: &'a Params,
        ordering: &'a MoveOrdering,
        multipv_count: usize,
        tt: &'a mut TranspositionTable,
    ) -> Self {
        Self {
            params,
            ordering,
            pv: PvTable::new(64),
            stats: SearchStats::default(),
            multipv: MultiPv::new(multipv_count),
            repetition_stack: Vec::with_capacity(256),
            tt ,
        }
    }
    #[inline(always)]
    pub fn is_threefold(&mut self, pos: &Chess) -> bool {

        let mut count = 0;

        let current = self.repetition_stack.last().unwrap_or(&0);
        let len = self.repetition_stack.len();

        if len == 0{
            return false;
        }

        // Avoid underflow
        let start = len.saturating_sub(pos.halfmoves() as usize + 1);

        // Scan backwards skipping last position
        for &hash in self.repetition_stack[start..len-1].iter().rev() {

            if hash == *current {
                count += 1;
                if count >= 2 {
                    return true; // 3-fold repetition
                }
            }
        }

        false
    }
    #[inline(always)]
    pub fn is_50_moves(&self,pos: &Chess) -> bool {
        pos.halfmoves()> 100
    }
    #[inline(always)]
    pub fn _init_history(&mut self, hash : u64) {
        self.repetition_stack.clear();
        self.repetition_stack.push(hash);
    }

    #[inline(always)]
    pub fn increase_history(&mut self, hash : u64) {
        self.repetition_stack.push(hash);
    }

    #[inline(always)]
    pub fn decrease_history(&mut self) {
        self.repetition_stack.pop();
    }


}
