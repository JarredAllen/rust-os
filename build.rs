use std::{env, fs, path::PathBuf};

fn main() {
    // Set the linker script
    let linker_script = "kernel.ld";
    println!("cargo:rerun-if-changed={linker_script}");

    let out =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR env var not specified by cargo"));
    fs::copy(linker_script, out.join(linker_script))
        .expect("Failed to copy linker script to output directory");
    println!("cargo:rustc-link-search={}", out.display());
}
