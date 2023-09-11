# This justfile can be used with just: https://crates.io/crates/just

setup:
    cargo install --locked trunk

frontend-build:
    cd desktop/frontend; trunk build
frontend-release:
    cd desktop/frontend; trunk build --release
frontend-watch:
    cd desktop/frontend; trunk watch

build: frontend-build rust-build

release: frontend-release
    cargo build --release --all-features --bin millenium-player

rust-build:
    cargo build --bin millenium-player
rust-check-fmt:
    cargo fmt --check
rust-test:
    cargo test --all-features
rust-clippy:
    cargo clippy --all-features

test: rust-check-fmt rust-clippy frontend-build rust-test

run: frontend-build
    cargo run --bin millenium-player -- simple
run-hydrate: frontend-build
    cargo run --bin millenium-player -- simple ./test-data/hydrate/hydrate.mp3

clean:
    cd desktop/frontend; trunk clean
    cargo clean
