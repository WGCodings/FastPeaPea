♟️ RustChess Engine

A fast UCI-compatible chess engine written in Rust, focused on clean architecture, high performance, and incremental strength improvements.

This project is built as a learning-focused but performance-driven engine, with modern search techniques and efficient move generation at its core.

🚀 Features
✅ Core Engine

Bitboard-based move generation (via shakmaty)

Legal move generation

Perft validation support

UCI protocol compatible (works with Lichess bots, Cutechess, Arena, etc.)

🔎 Search

Iterative Deepening

Negamax with Alpha-Beta pruning

Quiescence Search

Move Ordering:

PV move priority

MVV-LVA capture sorting

Promotion ordering (Queen first)

Multi-PV support

Time management system

Depth-limited search (go depth X)

📊 Evaluation

Material evaluation

Piece-square tables

Mobility bonuses

Bishop pair bonus

Pawn structure heuristics:

Doubled pawns

Isolated pawns

Rook open/semi-open file bonus

Tempo bonus

Center control

🧪 Testing & Benchmarking

Built-in perft command

go depth benchmarking support

NPS, nodes, time reporting

Compatible with external benchmark scripts

Designed for regression testing and performance tracking

🛠 Language & Stack

Language: Rust (stable)

Move generation: shakmaty

Protocol: UCI (Universal Chess Interface)

Designed for performance (215M+ NPS in perft on modern hardware)

📦 Building

Make sure you have Rust installed:

rustup update


Clone the repository:

git clone https://github.com/yourusername/rustchess-engine.git
cd rustchess-engine


Build in release mode:

cargo build --release


The binary will be located at:

target/release/<engine-name>

▶️ Running the Engine

Run directly:

./target/release/<engine-name>


The engine speaks UCI.

Example manual session:

uci
isready
position startpos
go depth 6

🧪 Perft Testing

You can validate move generation using:

position startpos
perft 6


Output includes:

Total nodes

Time taken

NPS

This is useful for:

Regression testing

Performance tracking

Move generator validation

🤖 Using With Lichess

This engine is compatible with:

lichess-bot

Cutechess

Arena

BanksiaGUI

Any UCI-compatible GUI

Simply configure the binary path in your GUI or bot configuration.

📈 Benchmarking

The engine supports:

go depth X


which makes it ideal for automated benchmarking.

You can:

Compare multiple binaries

Track NPS changes

Detect performance regressions

Validate search changes

🎯 Project Goals

Maintain extremely high NPS

Add strength with minimal speed loss

Keep architecture clean and modular

Experiment with modern pruning/search techniques

Gradually move toward tournament-ready strength

🔮 Planned Improvements

Transposition Table

Null-move pruning

Late Move Reductions (LMR)

Killer & History heuristics

Aspiration windows

Improved king safety evaluation

Endgame scaling

Tapered evaluation

📚 Why Rust?

Rust provides:

Memory safety without GC

High performance

Fearless concurrency (future SMP search)

Excellent tooling (Cargo, Clippy, Rustfmt)

Perfect for building a modern chess engine.

🧠 Engine Philosophy

This project aims to balance:

Simplicity × Performance × Strength

Rather than prematurely adding complex evaluation features that reduce NPS, the focus is:

Strong search

Efficient move ordering

Cheap but high-impact evaluation terms

Search first. Eval second.

📜 License

MIT License (or choose your preferred license)

👤 Author

Built and maintained by [Your Name]
