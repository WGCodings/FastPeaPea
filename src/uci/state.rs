use shakmaty::Chess;
use std::sync::atomic::AtomicBool;

pub struct UciState {
    pub position: Chess,
    pub multipv: usize,
    pub _wtime: u64,
    pub _btime: u64,
    pub _winc: u64,
    pub _binc: u64,
    pub stop: AtomicBool,
}

impl UciState {
    pub fn new() -> Self {
        Self {
            position: Chess::default(),
            _wtime: 0,
            _btime: 0,
            _winc: 0,
            _binc: 0,
            multipv: 1,
            stop: AtomicBool::new(false),
        }
    }
}
