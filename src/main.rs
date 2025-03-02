use std::collections::HashSet;
use std::env::{args, current_dir};
use std::fs::read_dir;
use std::io::ErrorKind;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command};
use std::str::FromStr;

use anyhow::{Context, Error, Result};
use clap::{Parser, ValueEnum};

const DEFAULT_SKIP_DIR_NAMES: [&str; 3] = [".git", ".rustup", ".cargo"];

fn main() -> Result<()> {
    let mut args = args();
    if let Some("clean-recursive") = std::env::args().skip(1).next().as_deref() {
        args.next();
    }
    let args = Args::parse_from(args);
    args.run()
}

#[derive(Debug, Parser)]
struct Args {
    /// Deletes documents
    #[clap(short, long)]
    doc: bool,

    /// Deletes release target
    #[clap(short = 'r', long)]
    release: bool,

    /// Display what would be deleted without actually deleting anything
    #[clap(short = 'n', long)]
    dry_run: bool,

    /// Recursive search depth limit
    #[clap(long, default_value_t = 64)]
    depth: usize,

    /// Skip scan directories with specified names. (if empty, '.git' '.rustup' '.cargo')
    #[clap(long)]
    skips: Option<Vec<String>>,

    /// How to handle IO errors.
    #[clap(long, default_value = "raise-unexpected")]
    io_error_handling: IoErrorHandling,

    /// Verbose mode
    #[clap(short = 'v', long)]
    verbose: bool,

    /// Target directory
    path: Option<PathBuf>,
}

impl Args {
    fn run(&self) -> Result<()> {
        let delete_mode = DeleteMode {
            doc: self.doc,
            release: self.release,
            dry_run: self.dry_run,
        };

        let skips: HashSet<String> = if let Some(ref skips) = self.skips {
            skips.iter().cloned().collect()
        } else {
            let mut skips = HashSet::new();
            for n in DEFAULT_SKIP_DIR_NAMES {
                skips.insert(n.to_string());
            }
            skips
        };

        let depth = self.depth;

        let path = if let Some(path) = self.path.clone() {
            path
        } else {
            current_dir().context("getting current_dir")?
        };

        let mut executions = Vec::new();

        process_dir(
            path,
            depth,
            &skips,
            delete_mode,
            self.io_error_handling,
            &mut executions,
        )?;

        let mut sum = bytesize::ByteSize::b(0);

        // Wait for all children to finish and sum up the space saved
        for CargoCleanExecution { child, path } in executions {
            match child.wait_with_output() {
                Ok(output) => {
                    // We only care if the command was successfully finished.
                    // Cargo may fail to clean due to various reasons.
                    //   (eg. too old format version of Cargo.toml, missing permission, etc.)
                    // We don't care about them.
                    if output.status.success() {
                        // cargo clean's output gets piped to stdout for some reason
                        let output = String::from_utf8_lossy(&output.stderr);
                        let output = output.trim();

                        // If verbose mode is enabled, print the output.
                        if self.verbose {
                            eprintln!("==== {} ====\n{}", path.display(), output);
                        }

                        // Get the first line of the cargo's output.
                        let output = output
                            .split_once('\n')
                            .map(|(first_line, _)| first_line)
                            .unwrap_or(output);

                        // If project is already clean, we don't need to parse size.
                        if self.dry_run {
                            // If cargo prints "Summary 0 files", we don't need to parse it.
                            if output == "Summary 0 files" {
                                continue;
                            }
                        } else {
                            // If cargo prints "Removed 0 files", we don't need to parse it.
                            if output == "Removed 0 files" {
                                continue;
                            }
                        }

                        // upon a non-empty cargo clean, we find how much data was removed.
                        // The 3rd item is the data amount (eg 7MiB)
                        //
                        // Example cargo's output:
                        //   Removed 2020 files, 986.5MiB total
                        let size = output
                            .split_whitespace()
                            .nth(3)
                            .map(bytesize::ByteSize::from_str);

                        match size {
                            Some(Ok(size)) => {
                                sum += size;
                            }
                            _ => {
                                eprintln!("Failed to parse size of cargo clean output: {}", output);
                            }
                        }
                    }
                }
                // If we failed to get the output, we just print the error.
                //
                // Erors may occur if the child process was started but not finished.
                // We can't do anything about it.
                Err(e) => {
                    eprintln!("Failed to get child process output: {}", e);
                }
            }
        }

        if self.dry_run {
            eprintln!("Total space that will be saved: {sum}");
        } else {
            eprintln!("Total space saved: {sum}");
        }

        Ok(())
    }
}

fn process_dir(
    path: PathBuf,
    depth: usize,
    skips: &HashSet<String>,
    del_mode: DeleteMode,
    io_error_handling: IoErrorHandling,
    executions: &mut Vec<CargoCleanExecution>,
) -> Result<()> {
    if depth == 0 {
        return Ok(());
    }

    if let Some(Some(dir_name)) = path.file_name().map(|n| n.to_str()) {
        if skips.contains(dir_name) {
            return Ok(());
        }
    }

    detect_and_clean(&path, del_mode, executions)
        .with_context(|| format!("cleaning directory {}", path.display()))?;

    let rd = match read_dir(&path)
        .handle_io_error(io_error_handling)
        .with_context(|| format!("reading directory {}", path.display()))?
    {
        ControlFlow::Continue(rd) => rd,
        ControlFlow::Break(()) => return Ok(()),
    };

    for entry in rd {
        let entry = match entry
            .handle_io_error(io_error_handling)
            .with_context(|| format!("reading directory entry {}", path.display()))?
        {
            ControlFlow::Continue(entry) => entry,
            ControlFlow::Break(()) => continue,
        };

        if entry.file_type()?.is_dir() {
            if let Err(e) = process_dir(
                entry.path(),
                depth - 1,
                skips,
                del_mode,
                io_error_handling,
                executions,
            ) {
                eprintln!("{:#}", e);
            }
        }
    }

    Ok(())
}

fn detect_and_clean(
    path: &Path,
    del_mode: DeleteMode,
    executions: &mut Vec<CargoCleanExecution>,
) -> Result<()> {
    let is_cargo_dir = path.join("Cargo.toml").is_file();
    if !is_cargo_dir {
        return Ok(());
    }

    eprintln!("Checking {:?}", path);

    let mut args = Vec::<&'static str>::new();

    if del_mode.do_release() {
        args.push("--release");
    }
    if del_mode.do_doc() {
        args.push("--doc");
    }
    if del_mode.dry_run {
        args.push("--dry-run");
    }

    executions.push(spawn_cargo_clean(path, &args)?);

    Ok(())
}

fn spawn_cargo_clean(current_dir: &Path, args: &[&str]) -> Result<CargoCleanExecution> {
    let child = Command::new("cargo")
        .arg("clean")
        .args(args)
        .current_dir(current_dir)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::piped())
        .spawn()
        .context("failed to spawn `cargo clean`")?;

    Ok(CargoCleanExecution {
        child,
        path: current_dir.to_path_buf(),
    })
}

#[derive(Debug)]
struct CargoCleanExecution {
    child: Child,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct DeleteMode {
    doc: bool,
    release: bool,
    dry_run: bool,
}

impl DeleteMode {
    fn do_doc(self) -> bool {
        self.doc
    }

    fn do_release(self) -> bool {
        self.release
    }
}

/// How to handle IO errors.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum IoErrorHandling {
    /// Ignore All IO errors.
    Ignore,

    /// Show only unexpected IO errors.
    ///
    /// For examples, "Permission denied" is an expected error.
    /// It may occur when the program tries to read a file that
    /// the user doesn't have permission to read.
    RaiseUnexpected,

    /// Print all IO errors.
    RaiseAll,
}

trait IoErrorHandlingExt<T> {
    fn handle_io_error(self, handling: IoErrorHandling) -> Result<ControlFlow<(), T>>;
}

impl<T> IoErrorHandlingExt<T> for std::result::Result<T, std::io::Error> {
    fn handle_io_error(self, handling: IoErrorHandling) -> Result<ControlFlow<(), T>> {
        match self {
            Ok(v) => Ok(ControlFlow::Continue(v)),
            Err(e) => match handling {
                IoErrorHandling::Ignore => Ok(ControlFlow::Break(())),
                IoErrorHandling::RaiseUnexpected => match e.kind() {
                    ErrorKind::PermissionDenied => Ok(ControlFlow::Break(())),
                    _ => Err(Error::from(e)),
                },
                IoErrorHandling::RaiseAll => Err(Error::from(e)),
            },
        }
    }
}
