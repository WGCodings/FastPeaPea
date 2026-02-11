use crate::engine::search::{
    ordering::MoveOrdering,
    pv::PvTable,
};
use crate::engine::params::Params;
use crate::engine::search::pv::MultiPv;
use crate::engine::search::search::SearchStats;

pub struct SearchContext<'a> {
    pub params: &'a Params,
    pub ordering: &'a MoveOrdering,
    pub pv: PvTable,
    pub stats: SearchStats,
    pub multipv: MultiPv,
}
