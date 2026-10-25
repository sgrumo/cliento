//! Generates the Swift bindings of `clientmail-ffi`. Run by `scripts/build-core-bridge.sh`.

fn main() {
    uniffi::uniffi_bindgen_main();
}
