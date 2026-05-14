use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../openapi.yaml");

    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let out_dir = Path::new(&out_dir);
    let generated_dir = out_dir.join("generated");

    if generated_dir.exists() {
        fs::remove_dir_all(&generated_dir)?;
    }
    fs::create_dir_all(&generated_dir)?;

    let spec = fs::read_to_string("../openapi.yaml")?;
    for file in satay_codegen::generate(&spec)? {
        let path = generated_dir.join(file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, file.contents)?;
    }

    fs::write(out_dir.join("satay_generated.rs"), "pub mod generated;\n")?;

    Ok(())
}
