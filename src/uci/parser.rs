use shakmaty::{CastlingMode, Chess, Move};
use shakmaty::uci::UciMove;

pub enum UciCommand {
    Uci,
    IsReady,
    UciNewGame,
    Position {
        fen: Option<String>,
        moves: Vec<String>,
    },
    Go {
        wtime: Option<u64>,
        btime: Option<u64>,
        movetime: Option<u64>,
        winc: Option<u64>,
        binc: Option<u64>,
        depth: Option<u32>,
    },
    Stop,
    Quit,
    SetOption {
        name: String,
        value: String,
    },
    Perft {
        depth: u32,
    },
    Unknown,
}

pub fn parse_command(input: &str) -> UciCommand {
    let tokens: Vec<&str> = input.trim().split_whitespace().collect();

    if tokens.is_empty() {
        return UciCommand::Unknown;
    }

    match tokens[0] {
        "uci" => UciCommand::Uci,
        "isready" => UciCommand::IsReady,
        "ucinewgame" => UciCommand::UciNewGame,
        "stop" => UciCommand::Stop,
        "quit" => UciCommand::Quit,
        "position" => {
            let mut fen = None;
            let mut moves = Vec::new();
            let mut i = 1;

            if tokens.get(i) == Some(&"startpos") {
                i += 1;
            } else if tokens.get(i) == Some(&"fen") {
                fen = Some(tokens[i + 1..i + 7].join(" "));
                i += 7;
            }

            if tokens.get(i) == Some(&"moves") {
                i += 1;
                while i < tokens.len() {
                    moves.push(tokens[i].to_string());
                    i += 1;
                }
            }

            UciCommand::Position { fen, moves }
        }
        "go" => {
            let mut wtime = None;
            let mut btime = None;
            let mut movetime = None;
            let mut winc = None;
            let mut binc = None;
            let mut depth = None;

            let mut i = 1;

            while i + 1 < tokens.len() {
                match tokens[i] {
                    "wtime" => wtime = tokens[i + 1].parse().ok(),
                    "btime" => btime = tokens[i + 1].parse().ok(),
                    "movetime" => movetime = tokens[i + 1].parse().ok(),
                    "winc" => winc = tokens[i + 1].parse().ok(),
                    "binc" => binc = tokens[i + 1].parse().ok(),
                    "depth" => depth = tokens[i + 1].parse().ok(),
                    _ => {}
                }
                i += 2;
            }

            UciCommand::Go {
                wtime,
                btime,
                movetime,
                winc,
                binc,
                depth,
            }
        }
        "setoption" => {
            let mut name = String::new();
            let mut value = String::new();
            let mut i = 1;

            while i < tokens.len() {
                match tokens[i] {
                    "name" => {
                        i += 1;
                        while i < tokens.len() && tokens[i] != "value" {
                            if !name.is_empty() {
                                name.push(' ');
                            }
                            name.push_str(tokens[i]);
                            i += 1;
                        }
                    }
                    "value" => {
                        i += 1;
                        while i < tokens.len() {
                            if !value.is_empty() {
                                value.push(' ');
                            }
                            value.push_str(tokens[i]);
                            i += 1;
                        }
                    }
                    _ => i += 1,
                }
            }

            UciCommand::SetOption { name, value }
        }
        "perft" => {
            if tokens.len() >= 2 {
                if let Ok(depth) = tokens[1].parse::<u32>() {
                    return UciCommand::Perft { depth };
                }
            }

            UciCommand::Perft { depth: 1 }
        }

        _ => UciCommand::Unknown,
    }
}

pub fn move_to_uci(mv: &Move) -> String {
    mv.to_uci(CastlingMode::Standard).to_string()
}

pub fn uci_to_move(pos: &Chess, s: &str) -> Move {
    let uci: UciMove = s.parse().unwrap();
    uci.to_move(pos).unwrap()
}
