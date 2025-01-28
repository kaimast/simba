BUILDTYPE := "debug"
LOG_LEVEL := "info"

CLIPPY_FLAGS := "-- --deny \"warnings\""

BUILD_FLAGS := if BUILDTYPE == "release" { "--release" } else { "" }
WASM_FLAGS := if BUILDTYPE == "release" { "--release" } else { "--dev" }

all: build #generate-web

install: build
    cargo install {{BUILD_FLAGS}} --path=./simba-cmd --locked
    cargo install {{BUILD_FLAGS}} --path=./native-gui --locked

lint: lint-cmd lint-native validate-shaders #lint-web
build: build-native build-cmd #build-web

check:
    cargo check --package=simba-native-gui
    cargo check --package=simba-cmd

validate-shaders:
    cd visualizer && bash ./validate-shaders.sh

lint-simba:
    cargo clippy --package=simba {{CLIPPY_FLAGS}}

lint-scripts:
    pylint plot.py plot_stats.py

lint-cmd:
    cargo clippy --package=simba-cmd {{CLIPPY_FLAGS}}

lint-native:
    cargo clippy --package=simba-native-gui {{CLIPPY_FLAGS}}

lint-web:
    env CARGO_TARGET_DIR=wasm-target cargo clippy --target=wasm32-unknown-unknown --package=simba-web-gui {{CLIPPY_FLAGS}}

unit-tests:
    env RUST_LOG=debug cargo test {{BUILD_FLAGS}} --package=simba --features=all

test: unit-tests test-bottleneck test-split test-ethereum

test-bottleneck: build-cmd
    RUST_LOG={{LOG_LEVEL}} RUST_BACKTRACE=1 ./target/{{BUILDTYPE}}/simba test bottleneck

test-split: build-cmd
    RUST_LOG={{LOG_LEVEL}} RUST_BACKTRACE=1 ./target/{{BUILDTYPE}}/simba test split

test-ethereum: build-cmd
    RUST_LOG={{LOG_LEVEL}} RUST_BACKTRACE=1 ./target/{{BUILDTYPE}}/simba test ethereum

test-pbft: build-cmd
    RUST_LOG={{LOG_LEVEL}} RUST_BACKTRACE=1 ./target/{{BUILDTYPE}}/simba test pbft

build-native: validate-shaders
    cargo build --package=simba-native-gui {{BUILD_FLAGS}}

build-cmd:
    cargo build --package=simba-cmd {{BUILD_FLAGS}}

build-cmd-static:
    env RUSTC=./rustc-static.wrap cargo build --verbose --package=simba-cmd {{BUILD_FLAGS}}

build-cmd-profiler:
    cargo build --package=simba-cmd {{BUILD_FLAGS}} --features=cpuprofiler

cloc:
    cloc --exclude-dir=wasm-target,target,out,build,pkg .

clean:
    cargo clean

generate-web: build-web
    mkdir -p out
    rsync -r web-gui/pkg out/
    rsync static/* out/

build-web: validate-shaders
    #! /bin/env bash
    DIR=`pwd`/wasm-target
    CARGO_TARGET_DIR=$DIR wasm-pack build --target=web {{WASM_FLAGS}} ./web-gui

fix-formatting:
    cargo fmt

update-dependencies:
    cargo update
