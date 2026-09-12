//! Pins the native stack the `.wasm` is linked with.
//!
//! This is the Rust shadow stack in linear memory, which a wasm engine cannot grow. An
//! overflow is a trap with no span, and it leaves the stack pointer where the trap found
//! it, so every later call into the same instance traps too. The parser's nesting cap
//! (`MAX_DEPTH`) and the VM's native re-entry cap (`MAX_NATIVE_DEPTH`) are sized so their
//! recursion fits in it.
//! Both flags below are what rustc's wasm32 target already passes, and wasm-ld's own
//! default is 64 KiB: stating them here makes the size the caps were measured against a
//! setting of this repo rather than a toolchain default. Cargo passes them after rustc's
//! own, and the linker keeps the last.
//!
//! `--stack-first` puts the stack below static data, so an overflow runs off the bottom
//! of memory and traps rather than overwriting that data.
//!
//! A link argument reaches every target of this package that links, the wasm32 test
//! binary included, so `npm run rust:wasm:test` runs its two depth-cap tests on this
//! stack. Those tests are what fail if the size ever drops under what the caps need. In
//! the debug build they run, each cap's deepest nesting needs more than 256 KiB and less
//! than 384 KiB, so a megabyte leaves both caps better than twice what they use.

const STACK_SIZE: usize = 1024 * 1024;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    // A host build links with its own linker, which does not take wasm-ld's flags, and
    // runs on the host's stack.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo::rustc-link-arg=-zstack-size={STACK_SIZE}");
        println!("cargo::rustc-link-arg=--stack-first");
    }
}
