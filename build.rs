// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, fs, path::Path};

fn sources(path: &Path, files: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read source directory") {
            sources(&entry.expect("read source entry").path(), files);
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        files.push(path.to_owned());
    }
}

fn main() {
    println!("cargo::rerun-if-env-changed=VYNULL_TOUCH_BUILD_ID");
    println!("cargo::rerun-if-changed=src");
    let mut files = Vec::new();
    sources(Path::new("src"), &mut files);
    for name in [
        "build.rs",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
    ] {
        println!("cargo::rerun-if-changed={name}");
        files.push(name.into());
    }
    files.sort();
    // A stable development source fingerprint, not an artifact integrity hash.
    // Releases use their manifest's build ID and a separate SHA-256 checksum.
    let mut fingerprint = 0xcbf29ce484222325_u64;
    for file in files {
        for byte in file
            .to_string_lossy()
            .bytes()
            .chain([0])
            .chain(fs::read(&file).expect("read source"))
        {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    let id =
        env::var("VYNULL_TOUCH_BUILD_ID").unwrap_or_else(|_| format!("dev-{fingerprint:016x}"));
    assert!(
        !id.is_empty()
            && id.len() <= 96
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
        "Invalid VYNULL_TOUCH_BUILD_ID"
    );
    println!("cargo::rustc-env=VYNULL_TOUCH_BUILD_ID={id}");
    println!(
        "cargo::rustc-env=VYNULL_TOUCH_TARGET={}",
        env::var("TARGET").unwrap()
    );
}
