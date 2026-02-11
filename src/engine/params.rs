
#[derive(Clone)]
pub struct Params {
    pub piece_values: [f32; 6],
    pub _pst: [[f32; 64]; 6],
    pub material_weight: f32,
    pub _pst_weight: f32,
    pub mobility_bonus: [i32; 6],
    pub tempo_bonus: f32
}

impl Default for Params {
    fn default() -> Self {
        Self {
            piece_values: [100.0, 320.0, 330.0, 500.0, 900.0, 0.0], // P, N, B, R, Q, K
            _pst: [[0.0; 64]; 6], // initialize later or fill with tuning values
            material_weight: 1.0,
            _pst_weight: 1.0,
            mobility_bonus:  [0, 3, 2, 2, 0, 0],
            tempo_bonus: 10.0
        }
    }
}
