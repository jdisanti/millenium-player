# This justfile can be used with just: https://crates.io/crates/just

setup:
    cd millenium-assets; npm install

build-assets:
    cd millenium-assets; npm run build

build: build-assets
    cargo build millenium-desktop

release: build-assets
    cargo build --release millenium-desktop

watch:
    cd millenium-assets; npm run watch

test: build-assets
    cargo fmt --check
    cargo test --all-features
    cargo clippy --all-features

run-hydrate:
    cargo run --bin millenium-desktop -- simple ./test-data/hydrate/hydrate.mp3

clean:
    rm -rf ./millenium-assets/build/
    rm -rf ./millenium-assets/node_modules/
    rm -rf ./target/