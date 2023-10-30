check:
  cargo check --target wasm32-unknown-unknown

lint:
  cargo +nightly clippy --tests

test:
  cargo test
