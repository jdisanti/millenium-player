# This justfile can be used with just: https://crates.io/crates/just

setup:
    cd desktop/ui; npm install

ui-fix:
    cd desktop/ui; npm run prettier:fix && npm run lint:fix
ui-test:
    cd desktop/ui; npm run prettier && npm run lint
ui-build:
    cd desktop/ui; npm run build
ui-watch:
    cd desktop/ui; npm run watch

build: ui-build rust-build

release: ui-build
    cargo build --release --bin millenium-player

rust-build:
    cargo build --bin millenium-player
rust-check-fmt:
    cargo fmt --check
rust-test:
    cargo test --all-features
rust-clippy:
    cargo clippy --all-features

test: ui-test ui-build rust-check-fmt rust-test rust-clippy

run:
    cargo run --bin millenium-player -- simple
run-hydrate:
    cargo run --bin millenium-player -- simple ./test-data/hydrate/hydrate.mp3

clean:
    rm -rf ./desktop/ui/build/
    rm -rf ./desktop/ui/node_modules/
    rm -rf ./target/
