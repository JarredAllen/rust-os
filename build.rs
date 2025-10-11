//! Build script for the kernel.

use std::{env, fs, path::PathBuf};

const LINKER_SCRIPT: &str = include_str!("./kernel.ld");

fn main() {
    let linker_script_path =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR env var not specified by cargo"));
    fs::write(linker_script_path.join("linker.ld"), LINKER_SCRIPT)
        .expect("Failed to copy linker script to output directory");
    println!("cargo:rustc-link-search={}", linker_script_path.display());
}
