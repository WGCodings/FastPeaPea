use shakmaty::{Chess, Color, Position};
use std::fs;
use crate::engine::utility::read_position_from_fen;
use crate::nnue::network::{calculate_index, calculate_threat_index, get_bucket, role_index};

// =====================================================================================================================//
// TEST SUITE FOR THREAT INPUT INDEX TESTS
// =====================================================================================================================//
struct ThreatIndexTest {
    fen: String,
    expected_stm: Vec<usize>,
    expected_ntm: Vec<usize>,
}

fn _active_indices(pos: &Chess, perspective: Color) -> Vec<usize> {
    let board = pos.board();
    let (bucket, is_mirrored) = get_bucket(board, perspective);
    let mut out = Vec::new();

    for square in shakmaty::Square::ALL {
        if let Some(piece) = board.piece_at(square) {
            let side = if piece.color == Color::White { 0 } else { 1 };
            let sq = square.to_usize();
            let piece_type = role_index(piece.role);
            out.push(calculate_index(side, sq, piece_type, perspective, bucket, is_mirrored));
        }
    }

    for square in shakmaty::Square::ALL {
        if let Some(piece) = board.piece_at(square) {
            let enemy_occ = board.by_color(!piece.color);
            let attacks = board.attacks_from(square) & enemy_occ;
            let attacker_sq = square.to_usize();
            for target in attacks {
                out.push(calculate_threat_index(attacker_sq, target.to_usize(), perspective, is_mirrored));
            }
        }
    }

    out.sort_unstable();
    out
}

#[test]
fn _threat_index_test_suite() {
    let raw = fs::read_to_string("assets/fens_threat_indices.csv")
        .expect("failed to read assets/fens_threat_indices.csv");

    let tests = _parse_threat_index_test_suite(&raw);

    let mut passed = 0;

    for (i, test) in tests.iter().enumerate() {
        let pos = match read_position_from_fen(&test.fen) {
            Some(p) => p,
            None => {
                println!("Test {} FAILED: could not parse FEN {}", i, test.fen);
                continue;
            }
        };

        let stm = pos.turn();
        let actual_stm = _active_indices(&pos, stm);
        let actual_ntm = _active_indices(&pos, !stm);

        if actual_stm == test.expected_stm && actual_ntm == test.expected_ntm {
            passed += 1;
        } else {
            println!("Test {} FAILED", i);
            println!("FEN          : {}", test.fen);
            if actual_stm != test.expected_stm {
                println!("Expected STM : {:?}", test.expected_stm);
                println!("Got STM      : {:?}", actual_stm);
            }
            if actual_ntm != test.expected_ntm {
                println!("Expected NTM : {:?}", test.expected_ntm);
                println!("Got NTM      : {:?}", actual_ntm);
            }
            println!();
        }
    }

    println!("Passed {}/{} threat index tests", passed, tests.len());
}

fn _parse_threat_index_test_suite(input: &str) -> Vec<ThreatIndexTest> {
    input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, ';').collect();
            assert_eq!(parts.len(), 3, "malformed test line: {}", line);

            let fen = parts[0].trim().to_string();

            let parse_list = |field: &str, prefix: &str| -> Vec<usize> {
                let stripped = field.trim().strip_prefix(prefix).unwrap_or_else(|| panic!("expected {prefix} prefix in: {field}"));
                if stripped.is_empty() {
                    Vec::new()
                } else {
                    let mut v: Vec<usize> = stripped.split(',').map(|x| x.trim().parse().unwrap()).collect();
                    v.sort_unstable();
                    v
                }
            };

            let expected_stm = parse_list(parts[1], "STM:");
            let expected_ntm = parse_list(parts[2], "NTM:");

            ThreatIndexTest { fen, expected_stm, expected_ntm }
        })
        .collect()
}