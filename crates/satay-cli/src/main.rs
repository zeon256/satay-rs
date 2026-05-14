#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command as ProcessCommand};

use argh::FromArgs;

/// Satay command line interface.
#[derive(Debug, FromArgs)]
struct Args {
    #[argh(subcommand)]
    command: CliCommand,
}

/// Satay subcommands.
#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum CliCommand {
    Generate(GenerateCommand),
}

/// Generate Rust client code from an OpenAPI document.
#[derive(Debug, FromArgs)]
#[argh(subcommand, name = "generate")]
struct GenerateCommand {
    /// openAPI YAML or JSON input path.
    #[argh(option, short = 'i')]
    input: PathBuf,

    /// output Rust file, or directory receiving mod.rs.
    #[argh(option, short = 'o')]
    output: PathBuf,

    /// run rustfmt on the generated Rust file.
    #[argh(switch)]
    rustfmt: bool,
}

fn main() {
    let args = argh::from_env();
    if let Err(err) = run(args) {
        eprintln!("satay: {err}");
        process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    match args.command {
        CliCommand::Generate(command) => generate(&command.input, &command.output, command.rustfmt),
    }
}

fn generate(input: &Path, output: &Path, rustfmt: bool) -> Result<(), String> {
    let spec = fs::read_to_string(input)
        .map_err(|err| format!("failed to read `{}`: {err}", input.display()))?;
    let generated = satay_codegen::generate(&spec).map_err(|err| err.to_string())?;
    let output_file = output_file(output);

    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create `{}`: {err}", parent.display()))?;
    }

    fs::write(&output_file, generated)
        .map_err(|err| format!("failed to write `{}`: {err}", output_file.display()))?;

    if rustfmt {
        run_rustfmt(&output_file)?;
    }

    Ok(())
}

fn output_file(output: &Path) -> PathBuf {
    if output.extension() == Some(OsStr::new("rs")) {
        output.to_owned()
    } else {
        output.join("mod.rs")
    }
}

fn run_rustfmt(path: &Path) -> Result<(), String> {
    let status = ProcessCommand::new("rustfmt")
        .arg(path)
        .status()
        .map_err(|err| format!("failed to run rustfmt: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("rustfmt failed for `{}`", path.display()))
    }
}
