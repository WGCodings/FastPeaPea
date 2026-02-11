# ♟️ Fast Pea Pea

Warning this document is written by AI.

A fast, UCI-compatible chess engine written in **Rust**, focused on clean architecture, high performance, and incremental strength improvements.

This engine is built as a performance-driven learning project, combining efficient search techniques with lightweight evaluation for maximum speed.

---

## 🚀 Features

### 🧠 Core Engine
- Bitboard-based move generation (via `shakmaty`)
- Fully legal move generation
- UCI protocol compatible
- Multi-PV support
- Depth-based and time-based search
- Built-in `perft` command for validation

---

### 🔎 Search
- Iterative Deepening
- Negamax with Alpha-Beta pruning
- Quiescence Search
- Efficient Move Ordering:
  - PV move priority
  - MVV-LVA capture sorting
- Time management system
- `go depth X` support
- NPS / nodes / time reporting

---

### 📊 Evaluation
- Material balance
- Mobility bonuses
- Tempo bonus

---

## 🧪 Testing & Benchmarking

The engine includes:

- `perft <depth>` command
- `go depth <n>` for reproducible benchmarks
- NPS reporting
- Node count reporting
- Time measurement

---

## 🛠 Tech Stack

| Component | Technology |
|------------|------------|
| Language | Rust (stable) |
| Move Generation | `shakmaty` |
| Protocol | UCI |
| Build System | Cargo |

Performance-focused build in release mode.

---

## 📦 Building

Install Rust:

```bash
rustup update
```
```bash
git clone https://github.com/WGCodings/FastPeaPea.git
cargo build --release
```

## 📜 License

Fast Pea Pea is licensed under the [MIT license](https://opensource.org/licenses/MIT).
