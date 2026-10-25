#!/usr/bin/env bash
# Builds the Rust core as an XCFramework and generates its Swift bindings.
#
# Outputs (both gitignored):
#   apps/macos/Packages/ClientMailKit/Artifacts/CoreBridgeFFI.xcframework
#   apps/macos/Packages/ClientMailKit/Sources/CoreBridge/Generated/CoreBridge.swift
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
core="$root/core"
kit="$root/apps/macos/Packages/ClientMailKit"
target="aarch64-apple-darwin"
lib_dir="$core/target/$target/release"

# Must match the app's deployment target, or the linker warns about newer object files
# (rustc and the C compiler for the bundled SQLite both read this variable).
export MACOSX_DEPLOYMENT_TARGET="26.0"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo build --manifest-path "$core/Cargo.toml" --package clientmail-ffi --release --target "$target"

# Library mode reads the UniFFI metadata embedded in the compiled library and the crate's
# uniffi.toml, so the bindings always match the code that was just built. It runs
# `cargo metadata` in the current directory, hence the subshell inside `core/`.
(
    cd "$core"
    cargo run --quiet --package uniffi-bindgen -- \
        generate --library "$lib_dir/libclientmail_ffi.dylib" \
        --language swift --out-dir "$work/bindings"
)

mkdir -p "$work/headers"
cp "$work/bindings/CoreBridgeFFI.h" "$work/headers/"
cp "$work/bindings/CoreBridgeFFI.modulemap" "$work/headers/module.modulemap"

rm -rf "$kit/Artifacts/CoreBridgeFFI.xcframework"
mkdir -p "$kit/Artifacts"
xcodebuild -create-xcframework \
    -library "$lib_dir/libclientmail_ffi.a" \
    -headers "$work/headers" \
    -output "$kit/Artifacts/CoreBridgeFFI.xcframework" >/dev/null

mkdir -p "$kit/Sources/CoreBridge/Generated"
cp "$work/bindings/CoreBridge.swift" "$kit/Sources/CoreBridge/Generated/CoreBridge.swift"

echo "Core bridge ready: $kit/Artifacts/CoreBridgeFFI.xcframework"
