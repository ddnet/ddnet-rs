#!/bin/bash

build() {
    find $1 -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | while read dir; do echo "Building package: $dir"; cargo clippy -p "$dir"; done
}
build lib
build game
build examples/wasm-modules
build examples/lib-modules
