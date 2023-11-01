check:
  cargo check --target wasm32-unknown-unknown

lint:
  cargo +nightly clippy --tests

test:
  cargo test

fuzz:
  cargo test --features fuzzing --test fuzzing -- --nocapture
