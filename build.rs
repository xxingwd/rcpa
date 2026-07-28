use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    topcoat_tailwind::BuildConfig::new()
        .output(out_dir.join("tailwind.css"))
        .cwd(".")
        .render()
        .expect("Failed to build Tailwind CSS");

    println!("cargo:rerun-if-changed=src/topcoat_admin");
    println!("cargo:rerun-if-changed=static");
}
