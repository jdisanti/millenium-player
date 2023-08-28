# This justfile can be used with just: https://crates.io/crates/just

setup:
    cd millenium-assets; npm install

build-assets:
    cd millenium-assets; npm run build

build: build-assets
    cargo build --bin millenium-desktop

release: build-assets
    cargo build --release --bin millenium-desktop

watch:
    cd millenium-assets; npm run watch

rust-check-fmt:
    cargo fmt --check
rust-test:
    cargo test --all-features
rust-clippy:
    cargo clippy --all-features

test: build-assets rust-check-fmt rust-test rust-clippy

run:
    cargo run --bin millenium-desktop -- simple
run-hydrate:
    cargo run --bin millenium-desktop -- simple ./test-data/hydrate/hydrate.mp3

clean:
    rm -rf ./millenium-assets/build/
    rm -rf ./millenium-assets/node_modules/
    rm -rf ./target/
