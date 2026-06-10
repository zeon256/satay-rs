use std::env;
use std::fs;
use std::path::Path;
use std::process;

use satay_codegen::GeneratedFile;

pub const SIMPLE: &str = include_str!("../../../../tests/fixtures/simple.yaml");
pub const PETSTORE_MINIMAL: &str = include_str!("../../../../tests/fixtures/petstore-minimal.yaml");
pub const CONSTRAINED: &str = include_str!("../../../../tests/fixtures/constrained.yaml");
pub const INLINE_ENUM: &str = include_str!("../../../../tests/fixtures/inline-enum.yaml");
pub const RESPONSE_NAME_COLLISION: &str = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      responses:
        '200':
          description: PSI readings
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/PsiResponse'
components:
  schemas:
    PsiResponse:
      type: object
      required:
        - value
      properties:
        value:
          type: integer
"#;

pub fn find_file<'a>(files: &'a [GeneratedFile], relative_path: &str) -> &'a GeneratedFile {
    files
        .iter()
        .find(|f| f.relative_path == relative_path)
        .unwrap_or_else(|| {
            panic!(
                "expected file {relative_path}, found: {:?}",
                files.iter().map(|f| &f.relative_path).collect::<Vec<_>>()
            )
        })
}

pub fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("codegen crate has workspace root")
}

pub fn runtime_path_toml() -> String {
    toml_string(
        &workspace_root()
            .join("crates/satay-runtime")
            .to_string_lossy(),
    )
}

pub fn run_temp_cargo(crate_dir: &Path, subcommand: &str, extra_args: &[&str], context: &str) {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    prepare_temp_lock(crate_dir, &cargo, context);

    let output = process::Command::new(&cargo)
        .arg(subcommand)
        .arg("--locked")
        .arg("--offline")
        .arg("--quiet")
        .args(extra_args)
        .current_dir(crate_dir)
        .output()
        .unwrap_or_else(|err| panic!("run cargo {subcommand} for {context}: {err}"));

    assert!(
        output.status.success(),
        "{context} failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn prepare_temp_lock(crate_dir: &Path, cargo: &str, context: &str) {
    let lock_path = crate_dir.join("Cargo.lock");
    if !lock_path.exists() {
        // Seed resolved versions so offline lockfile generation does not need registry index data.
        fs::copy(workspace_root().join("Cargo.lock"), &lock_path)
            .expect("copy workspace Cargo.lock");
    }

    let output = process::Command::new(cargo)
        .arg("generate-lockfile")
        .arg("--offline")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .unwrap_or_else(|err| panic!("prepare lockfile for {context}: {err}"));

    assert!(
        output.status.success(),
        "{context} lockfile preparation failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output = process::Command::new(cargo)
        .arg("fetch")
        .arg("--locked")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .unwrap_or_else(|err| panic!("fetch dependencies for {context}: {err}"));

    assert!(
        output.status.success(),
        "{context} dependency fetch failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn write_manifest(
    crate_dir: &Path,
    runtime_path: &str,
    constrained: bool,
    _for_compile_test: bool,
) {
    let nutype_deps = if constrained {
        r#"
nutype = { version = "0.7", features = ["serde", "regex"] }
regex = "1"
"#
    } else {
        ""
    };

    let manifest = format!(
        r#"[package]
name = "satay-generated-check"
version = "0.0.0"
edition = "2024"

[features]
default = ["serde", "json"]
serde = ["dep:serde", "satay-runtime/serde"]
json = ["serde", "dep:serde_json", "satay-runtime/json"]

[dependencies]
http = "1"
satay-runtime = {{ path = {runtime_path}, default-features = false }}
serde = {{ version = "1", features = ["derive"], optional = true }}
serde_json = {{ version = "1", optional = true }}
{nutype_deps}"#
    );
    fs::create_dir_all(crate_dir.join("src")).expect("create src dir");
    fs::write(crate_dir.join("Cargo.toml"), manifest).expect("write manifest");
}

pub fn write_generated_files(generated_dir: &Path, files: &[GeneratedFile]) {
    for file in files {
        let path = generated_dir.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create generated subdir");
        }
        fs::write(&path, &file.contents).expect("write generated file");
    }
}

pub fn toml_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
