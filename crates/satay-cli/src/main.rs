#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command as ProcessCommand, ExitStatus};

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

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("failed to read `{}`: {source}", path.display())]
    ReadInput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error(transparent)]
    Generate(#[from] satay_codegen::Error),

    #[error("failed to create `{}`: {source}", path.display())]
    CreateOutputDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to write `{}`: {source}", path.display())]
    WriteOutput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to run rustfmt: {source}")]
    RunRustfmt {
        #[source]
        source: io::Error,
    },

    #[error("rustfmt failed for `{}` with status {status}", path.display())]
    RustfmtFailed { path: PathBuf, status: ExitStatus },
}

fn main() {
    let args = argh::from_env();
    if let Err(err) = run(args) {
        eprintln!("satay: {err}");
        process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Error> {
    match args.command {
        CliCommand::Generate(command) => generate(&command.input, &command.output, command.rustfmt),
    }
}

fn generate(input: &Path, output: &Path, rustfmt: bool) -> Result<(), Error> {
    let spec = fs::read_to_string(input).map_err(|source| Error::ReadInput {
        path: input.to_owned(),
        source,
    })?;
    let generated = satay_codegen::generate(&spec)?;
    let output_file = output_file(output);

    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::CreateOutputDir {
            path: parent.to_owned(),
            source,
        })?;
    }

    fs::write(&output_file, generated).map_err(|source| Error::WriteOutput {
        path: output_file.clone(),
        source,
    })?;

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

fn run_rustfmt(path: &Path) -> Result<(), Error> {
    let status = ProcessCommand::new("rustfmt")
        .arg(path)
        .status()
        .map_err(|source| Error::RunRustfmt { source })?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::RustfmtFailed {
            path: path.to_owned(),
            status,
        })
    }
}
