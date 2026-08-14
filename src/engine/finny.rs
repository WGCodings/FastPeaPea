use shakmaty::{Bitboard, Color};
use crate::nnue::network::{Accumulator, Network, NUM_INPUT_BUCKETS};



#[derive(Clone, Copy)]
pub struct FinnyEntry {
    pub acc: Accumulator,
    pub piece_bb: [[Bitboard; 6]; 2]
}

impl FinnyEntry {
    fn default(net: &Network) -> Self {
        Self { acc: Accumulator::new(net), piece_bb: [[Bitboard::EMPTY; 6]; 2] }
    }
}

pub struct FinnyTable {
    entries: [FinnyEntry; NUM_INPUT_BUCKETS * 2],
}

impl FinnyTable {
    pub(crate) fn default(net: &Network) -> Self {
        Self { entries: std::array::from_fn(|_| FinnyEntry::default(net)) }
    }

    /// Get finny entry. 0 - NUM_INPUT_BUCKETS-1 for black and the rest for white finny tables
    pub(crate) fn get_entry(&mut self, perspective: Color, bucket: usize) -> &mut FinnyEntry {
        &mut self.entries[usize::from(perspective) * NUM_INPUT_BUCKETS + bucket]
    }
}