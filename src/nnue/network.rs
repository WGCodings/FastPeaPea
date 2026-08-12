const HIDDEN_SIZE: usize = 1536;
const SCALE: i32 = 400;
const QA: i16 = 255;
const QB: i16 = 64;

const NUM_OUTPUT_BUCKETS : usize = 8;

const KING_BUCKET_LAYOUT: [usize; 32] = [
    0, 0, 1, 1,
    2, 2, 4, 4,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
];
const NUM_INPUT_BUCKETS: usize = 5;

use std::arch::x86_64::*;

use shakmaty::{Board, Chess, Color, Position, Role};

static NNUE: Network = unsafe { std::mem::transmute(*include_bytes!("../../nnue/files/kb/quantised.bin")) };

// =====================================================================================================================//
// NNUE NETWORK IS TRAINED BY THE BULLET CRATE AND CODE HAS BEEN REUSED FROM ONE OF THE EXAMPLES TO DO THE INFERENCE
// =====================================================================================================================//
///Contains info about the state of the accumulator.
/// It says if the accumulator is mirrored and what king bucket is currently being used.
#[derive(Clone, Copy)]
pub struct BucketInfo {
    pub mirror: bool,
    pub bucket: usize,
}

/// Returns the bucketinfo for a given color/perspective and the current board.
fn get_bucket(board: &Board, perspective: Color) -> BucketInfo {
    let king_sq = board.king_of(perspective).unwrap();
    let mut sq_idx = king_sq.to_usize();

    let file = sq_idx & 0b111;
    let rank = sq_idx >> 3;
    let mirror = file >= 4;
    let bucket_file = if mirror { 7 - file } else { file };
    let bucket = KING_BUCKET_LAYOUT[rank * 4 + bucket_file];

    BucketInfo { mirror, bucket }
}

#[inline(always)]
pub fn calculate_index(mut side: usize, mut sq_idx: usize, piece_type: usize, perspective: Color, info: BucketInfo) -> usize {
    if perspective == Color::Black {
        side = 1 - side;
        sq_idx ^= 0b111000;
    }
    if info.mirror {
        sq_idx ^= 0b000111;
    }
    info.bucket * 768 + (side * 6 + piece_type) * 64 + sq_idx
}

#[inline(always)]
pub fn accumulator_for_perspective<P: Position>(pos: &P, net: &Network, perspective: Color) -> (Accumulator, BucketInfo) {
    let mut acc = Accumulator::new(net);
    let board = pos.board();
    let info = get_bucket(board, perspective);

    for square in shakmaty::Square::ALL {
        if let Some(piece) = board.piece_at(square) {
            let sq_idx = shakmaty::Square::to_usize(square);
            let piece_type = role_index(piece.role);
            let side = if piece.color == Color::White { 0 } else { 1 };
            acc.add_feature(calculate_index(side, sq_idx, piece_type, perspective, info), net);
        }
    }
    (acc, info)
}

#[inline(always)]
pub fn role_index(role: Role) -> usize {
    match role {
        Role::Pawn => 0,
        Role::Knight => 1,
        Role::Bishop => 2,
        Role::Rook => 3,
        Role::Queen => 4,
        Role::King => 5,
    }
}

#[inline]
/// Square Clipped ReLU - Activation Function.
/// Note that this takes the i16s in the accumulator to i32s.
/// Range is 0.0 .. 1.0 (in other words, 0 to QA*QA quantized).
pub fn screlu(x: i16) -> i32 {
    let y = i32::from(x).clamp(0, i32::from(QA));
    y * y
}

/// This is the quantised format that bullet outputs.
#[repr(C)]
pub struct Network {
    /// Column-Major `HIDDEN_SIZE x 768` matrix.
    /// Values have quantization of QA.
    feature_weights: [Accumulator; 768 * NUM_INPUT_BUCKETS],
    /// Vector with dimension `HIDDEN_SIZE`.
    /// Values have quantization of QA.
    feature_bias: Accumulator,
    /// Column-Major `1 x (2 * HIDDEN_SIZE)`
    /// matrix, we use it like this to make the
    /// code nicer in `Network::evaluate`.
    /// Values have quantization of QB.
    output_weights: [i16; 2 * HIDDEN_SIZE*NUM_OUTPUT_BUCKETS],
    /// Scalar output bias.
    /// Value has quantization of QA * QB.
    output_bias: [i16;NUM_OUTPUT_BUCKETS]
}

impl Network {
    /// Calculates the output of the network, starting from the already
    /// calculated hidden layer (done efficiently during makemoves).
    #[cfg(not(target_feature = "avx2"))]
    pub fn evaluate(&self, us: &Accumulator, them: &Accumulator, pos : &Chess) -> i32 {
        let mut output = 0;
        let bucket = self.bucket(pos);
        let offset = bucket * 2 * HIDDEN_SIZE;

        let us_weights = &self.output_weights[offset .. offset + HIDDEN_SIZE];
        let them_weights = &self.output_weights[offset + HIDDEN_SIZE .. offset + 2 * HIDDEN_SIZE];

        // Side-To-Move
        for (&input, &weight) in us.vals.iter().zip(us_weights) {
            output += screlu(input) * i32::from(weight);
        }

        // Not-Side-To-Move
        for (&input, &weight) in them.vals.iter().zip(them_weights) {
            output += screlu(input) * i32::from(weight);
        }

        output /= i32::from(QA);

        output += i32::from(self.output_bias[bucket]);

        output *= SCALE;

        output /= i32::from(QA) * i32::from(QB);

        output
    }

    #[cfg(target_feature = "avx2")]
    pub fn evaluate(&self, us: &Accumulator, them: &Accumulator, pos: &Chess) -> i32 {
        let bucket = self.bucket(pos);
        let offset = bucket * 2 * HIDDEN_SIZE;
        let us_weights = &self.output_weights[offset..offset + HIDDEN_SIZE];
        let them_weights = &self.output_weights[offset + HIDDEN_SIZE..offset + 2 * HIDDEN_SIZE];
        unsafe {
            let zero = _mm256_setzero_si256();
            let qa = _mm256_set1_epi16(QA);

            let sum = _mm256_add_epi32(Self::screlu_avx2(&us.vals, us_weights, zero, qa), Self::screlu_avx2(&them.vals, them_weights, zero, qa));

            let mut output = Self::hsum_epi32(sum);

            output /= i32::from(QA);
            output += i32::from(self.output_bias[bucket]);
            output *= SCALE;
            output /= i32::from(QA) * i32::from(QB);
            output
        }
    }

    #[cfg(target_feature = "avx2")]
    #[inline(always)]
    unsafe fn screlu_avx2(
        inputs: &[i16; HIDDEN_SIZE],
        weights: &[i16],
        zero: __m256i,
        qa: __m256i,
    ) -> __m256i {
        let mut acc = _mm256_setzero_si256();
        let in_ptr = inputs.as_ptr();
        let w_ptr = weights.as_ptr();

        for i in (0..HIDDEN_SIZE).step_by(16) {
            let x = _mm256_load_si256(in_ptr.add(i) as *const __m256i);
            let w = _mm256_loadu_si256(w_ptr.add(i) as *const __m256i);

            let clamped = _mm256_min_epi16(_mm256_max_epi16(x, zero), qa);
            let t = _mm256_mullo_epi16(clamped, w);
            let prod = _mm256_madd_epi16(clamped, t);

            acc = _mm256_add_epi32(acc, prod);
        }
        acc
    }
    #[cfg(target_feature = "avx2")]
    #[inline(always)]
    unsafe fn hsum_epi32(v: __m256i) -> i32 {
        let hi = _mm256_extracti128_si256(v, 1);
        let lo = _mm256_castsi256_si128(v);
        let sum128 = _mm_add_epi32(hi, lo);
        let hi64 = _mm_unpackhi_epi64(sum128, sum128);
        let sum64 = _mm_add_epi32(sum128, hi64);
        let hi32 = _mm_shuffle_epi32(sum64, 0b01);
        let sum32 = _mm_add_epi32(sum64, hi32);
        _mm_cvtsi128_si32(sum32)
    }

    pub fn load() -> &'static Network {
        &NNUE
    }

    pub fn _load_from_path(path: &str) -> Box<Network> {
        let bytes = std::fs::read(path)
            .unwrap_or_else(|e| panic!("Failed to read network '{}': {}", path, e));
        assert_eq!(
            bytes.len(),
            std::mem::size_of::<Network>(),
            "Network file '{}' has wrong size", path
        );
        unsafe {
            let mut net = Box::new(std::mem::zeroed::<Network>());
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                net.as_mut() as *mut Network as *mut u8,
                std::mem::size_of::<Network>(),
            );
            net
        }
    }

    fn bucket(&self, pos: &Chess) -> usize {
        let divisor = 32usize.div_ceil(NUM_OUTPUT_BUCKETS);
        (pos.board().occupied().count() - 2) / divisor
    }

    /*
    fn queen_bucket(&self, pos: &Chess) -> usize {
        // Non-pawn material count
        let board = pos.board();
        let pawn_count = board.pawns().count();
        let npm_count = board.occupied().count() - pawn_count;

        // N is NUM_OUTPUT_BUCKETS / 3
        const N: usize = NUM_OUTPUT_BUCKETS / 3;
        let divisor = 16usize.div_ceil(N);
        let material_bucket = ((npm_count - 2) / divisor).min(N - 1);

        // Queen bucket
        let queen_count = board.queens().count();
        let queen_bucket = queen_count.min(2);

        material_bucket * 3 + queen_bucket
    }
     */

}


/// A column of the feature-weights matrix.
/// Note the `align(64)`.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct Accumulator {
    pub(crate) vals: [i16; HIDDEN_SIZE],
}

impl Accumulator {
    /// Initialised with bias so we can just efficiently
    /// operate on it afterwards.
    pub fn new(net: &Network) -> Self {
        net.feature_bias
    }

    /// Add a feature to an accumulator.
    pub fn add_feature(&mut self, feature_idx: usize, net: &Network) {
        for (i, d) in self.vals.iter_mut().zip(&net.feature_weights[feature_idx].vals) {
            *i += *d
        }
    }

    /// Combine remove and add features per move into a list and do them in one go instead as one per one.
    pub fn apply_feature_updates(&mut self, adds: &[usize], removes: &[usize], net: &Network) {

        for &idx in adds {
            for (i, d) in self.vals.iter_mut().zip(&net.feature_weights[idx].vals) {
                *i += *d
            }
        }
        for &idx in removes {
            for (i, d) in self.vals.iter_mut().zip(&net.feature_weights[idx].vals) {
                *i -= *d
            }
        }
    }
}