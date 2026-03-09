use std::env;
use std::path::PathBuf;

fn main() {
    // Get the directory containing the Cargo.toml
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    
    // Tell cargo to pass the linker script to the linker with full path
    let linker_script = manifest_dir.join("linker.ld");
    println!("cargo:rustc-link-arg=-T{}", linker_script.display());
    
    // Tell cargo to invalidate the built crate whenever the linker script changes
    println!("cargo:rerun-if-changed=linker.ld");
}
