use shakmaty::Move;

#[derive(Clone, Copy)]
pub enum Bound {
    Exact,
    Lower, // beta cutoff
    Upper, // alpha cutoff
}

#[derive(Clone)]
pub struct TTEntry {
    pub key: u64,
    pub depth: u8,
    pub score: f32,
    pub bound: Bound,
    pub best_move: Option<Move>,
}
#[derive(Clone)]
pub struct TranspositionTable {
    table: Vec<Option<TTEntry>>,
    mask: usize,
    entries : usize,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let bytes = size_mb * 1024 * 1024;
        let entry_size = size_of::<Option<TTEntry>>();
        let mut capacity = bytes / entry_size;

        // force power of two
        capacity = capacity.next_power_of_two();

        Self {
            table: vec![None; capacity],
            mask: capacity - 1,
            entries : 0
        }
    }

    #[inline(always)]
    fn index(&self, key: u64) -> usize {
        key as usize & self.mask
    }

    #[inline(always)]
    pub fn probe(&self, key: u64) -> Option<&TTEntry> {
        let entry = &self.table[self.index(key)];
        if let Some(e) = entry {
            if e.key == key {
                return Some(e);
            }
        }
        None
    }

    #[inline(always)]
    pub fn store(
        &mut self,
        key: u64,
        depth: usize,
        score: f32,
        bound: Bound,
        best_move: Option<Move>,
    ) {
        let idx = self.index(key);

        if self.table[idx].is_none() {
            self.entries += 1;
        }

        if let Some(existing) = &self.table[idx] {
            if existing.depth > depth as u8 {
                return; // don't replace deeper entry
            }
        }

        let entry = TTEntry {
            key,
            depth: depth as u8,
            score,
            bound,
            best_move,
        };

        self.table[idx] = Some(entry);
    }
    pub fn clear(&mut self) {
        self.entries = 0;
        unsafe {
            std::ptr::write_bytes(
                self.table.as_mut_ptr(),
                0,
                self.table.len(),
            );
        }
    }
    pub fn tt_occupancy(&self) -> u32 {
        let used = self.entries as f64;
        let total = self.table.len() as f64;

        ((used / total) * 1000.0) as u32
    }
}
