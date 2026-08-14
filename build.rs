use std::env;

const DEFAULT_PATH: &str = "nnue/files/quantised.bin";
fn main() {
    println!("cargo:rerun-if-env-changed=EVALFILE");
    println!("cargo:rerun-if-changed={}", DEFAULT_PATH);

    let eval_file = env::var("EVALFILE").unwrap_or(DEFAULT_PATH.into());

    if eval_file != DEFAULT_PATH{
        std::fs::copy(&eval_file, DEFAULT_PATH).unwrap_or_else(|e| panic!("Failed to copy '{}' to '{}': {}", eval_file, DEFAULT_PATH, e));
    }
}