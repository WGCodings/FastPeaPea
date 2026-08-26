mod engine;
mod uci;
mod nnue;
mod datagen;
mod tests;

use crate::uci::handler::UciHandler;

fn main() {
    UciHandler::new().run();
}