use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn hash_path(path: &Path, hash: &mut DefaultHasher) -> io::Result<()> {
    path.hash(hash);
    if path.is_dir() {
        let mut entries = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        entries.sort();
        for entry in entries {
            hash_path(&entry, hash)?;
        }
    } else if path.is_file() {
        fs::read(path)?.hash(hash);
    }
    Ok(())
}

fn fingerprint(root: &Path, paths: &[&str]) -> io::Result<u64> {
    let mut hash = DefaultHasher::new();
    include_bytes!("build.rs").hash(&mut hash);
    for path in paths {
        hash_path(&root.join(path), &mut hash)?;
    }
    Ok(hash.finish())
}

fn bun(root: &Path, args: &[&str]) {
    let output = Command::new("bun")
        .args(args)
        .current_dir(root)
        .output()
        .expect("Bun is required to compile the frontend; install it and put it on PATH");
    assert!(
        output.status.success(),
        "Frontend command `bun {}` failed:\n{}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn main() {
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("missing manifest directory"));
    let inputs = [
        "frontend",
        "package.json",
        "bun.lock",
        "tsconfig.json",
        "vite.config.ts",
    ];
    for path in inputs.iter().chain(["dist"].iter()) {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=PATH");
    let stamp = PathBuf::from(env::var_os("OUT_DIR").expect("missing Cargo output directory"))
        .join("frontend.fingerprint");
    let input = fingerprint(&root, &inputs).expect("cannot read frontend sources");
    let output = fingerprint(&root, &["dist"]).expect("cannot read frontend assets");
    let expected = format!("{input}:{output}");
    // Check both sources and outputs so deleting dist or one of its assets rebuilds it.
    if !root.join("dist/index.html").is_file()
        || fs::read_to_string(&stamp).ok().as_deref() != Some(expected.as_str())
    {
        bun(&root, &["install", "--frozen-lockfile"]);
        bun(
            &root,
            &["--bun", "node_modules/typescript/bin/tsc", "--build"],
        );
        bun(&root, &["--bun", "node_modules/vite/bin/vite.js", "build"]);
        let output = fingerprint(&root, &["dist"]).expect("cannot read built frontend assets");
        fs::write(stamp, format!("{input}:{output}"))
            .expect("cannot save frontend build fingerprint");
    }
    // The explicit glob avoids Cargo repeatedly watching an absent capabilities directory.
    tauri_build::try_build(
        tauri_build::Attributes::new().capabilities_path_pattern("capabilities/**/*.json"),
    )
    .expect("cannot generate Tauri configuration");
}
