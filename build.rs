use std::{collections::hash_map::DefaultHasher, hash::Hasher, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    topcoat_tailwind::BuildConfig::new()
        .output(out_dir.join("tailwind.css"))
        .cwd(".")
        .render()
        .expect("Failed to build Tailwind CSS");

    for (name, path) in [
        ("MODEL_CATALOG_CSS", "static/admin/model-catalog.css"),
        ("MODEL_CATALOG_JS", "static/admin/model-catalog.js"),
    ] {
        let mut hash = DefaultHasher::new();
        hash.write(&std::fs::read(path).expect("Failed to read admin asset"));
        println!("cargo:rustc-env=RCPA_{name}_HASH={:x}", hash.finish());
    }

    println!("cargo:rerun-if-changed=src/topcoat_admin");
    println!("cargo:rerun-if-changed=static");
}
