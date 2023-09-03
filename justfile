# This justfile can be used with just: https://crates.io/crates/just

setup:
    cd millenium-assets; npm install

assets-fix:
    cd millenium-assets; npm run prettier:fix && npm run lint:fix
assets-test:
    cd millenium-assets; npm run prettier && npm run lint
assets-build:
    cd millenium-assets; npm run build

build: assets-build rust-build

release: assets-build
    cargo build --release --bin millenium-desktop

watch:
    cd millenium-assets; npm run watch

rust-build:
    cargo build --bin millenium-desktop
rust-check-fmt:
    cargo fmt --check
rust-test:
    cargo test --all-features
rust-clippy:
    cargo clippy --all-features

test: assets-test assets-build rust-check-fmt rust-test rust-clippy

run:
    cargo run --bin millenium-desktop -- simple
run-hydrate:
    cargo run --bin millenium-desktop -- simple ./test-data/hydrate/hydrate.mp3

clean:
    rm -rf ./millenium-assets/build/
    rm -rf ./millenium-assets/node_modules/
    rm -rf ./target/
