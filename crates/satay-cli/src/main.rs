#![forbid(unsafe_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command as ProcessCommand, ExitStatus};

use argh::FromArgs;
use satay_codegen::RootModule;
use tracing::{error, info, instrument};
use tracing_subscriber::EnvFilter;

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

    /// output directory for generated Rust modules.
    #[argh(option, short = 'o')]
    output: PathBuf,

    /// run rustfmt on each generated file.
    #[argh(switch)]
    rustfmt: bool,

    /// write the root module as `lib.rs` instead of `mod.rs`.
    #[argh(switch)]
    lib: bool,
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

    #[error("failed to create directory `{}`: {source}", path.display())]
    CreateDir {
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
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = argh::from_env();
    if let Err(err) = run(args) {
        error!("{err}");
        process::exit(1);
    }
}

#[instrument(err)]
fn run(args: Args) -> Result<(), Error> {
    match args.command {
        CliCommand::Generate(command) => generate(
            &command.input,
            &command.output,
            command.rustfmt,
            command.lib,
        ),
    }
}

#[instrument(fields(input = %input.display(), output = %output.display(), rustfmt = rustfmt, lib = lib), err)]
fn generate(input: &Path, output: &Path, rustfmt: bool, lib: bool) -> Result<(), Error> {
    info!("reading input file");
    let spec = fs::read_to_string(input).map_err(|source| Error::ReadInput {
        path: input.to_owned(),
        source,
    })?;
    info!("generating client code");
    let options = satay_codegen::GenerateOptions {
        root_module: if lib {
            RootModule::LibRs
        } else {
            RootModule::ModRs
        },
    };
    let files = satay_codegen::generate_with(&spec, options)?;

    let mut rustfmt_files = vec![];

    for file in &files {
        let path = output.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::CreateDir {
                path: parent.to_owned(),
                source,
            })?;
        }
        info!(path = %path.display(), "writing output file");
        fs::write(&path, &file.contents).map_err(|source| Error::WriteOutput {
            path: path.clone(),
            source,
        })?;
        if rustfmt {
            rustfmt_files.push(path);
        }
    }

    if !rustfmt_files.is_empty() {
        info!("running rustfmt");
        for path in &rustfmt_files {
            run_rustfmt(path)?;
        }
    }

    info!("done");
    Ok(())
}

#[instrument(fields(path = %path.display()), err)]
fn run_rustfmt(path: &Path) -> Result<(), Error> {
    let status = ProcessCommand::new("rustfmt")
        .arg("--edition")
        .arg("2024")
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
