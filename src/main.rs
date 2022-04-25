use std::collections::HashSet;
use std::env::{args, current_dir};
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command};

use anyhow::{Context, Result};
use clap::Parser;

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
#[clap(bin_name = "cargo clean-recursive")]
struct Args {
    /// Deletes documents
    #[clap(short, long)]
    doc: bool,
    /// Deletes release target
    #[clap(short, long)]
    release: bool,
    /// Recursive serarch depth limit
    #[clap(long, default_value_t = 64)]
    depth: usize,
    /// Skip directories with specified names. (if empty, '.git' and '.cargo')
    #[clap(long)]
    skips: Option<Vec<String>>,
    /// Target directory
    path: Option<PathBuf>,
}

impl Args {
    fn run(&self) -> Result<()> {
        let delete_mode = DeleteMode {
            doc: self.doc,
            release: self.release,
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

        let mut children = Vec::new();

        process_dir(path, depth, &skips, delete_mode, &mut children)?;

        for mut child in children {
            if let Err(e) = child.wait() {
                eprintln!("{:#}", e);
            }
        }

        Ok(())
    }
}

fn process_dir(
    path: PathBuf,
    depth: usize,
    skips: &HashSet<String>,
    del_mode: DeleteMode,
    children: &mut Vec<Child>,
) -> Result<()> {
    if depth == 0 {
        return Ok(());
    }

    if let Some(Some(dir_name)) = path.file_name().map(|n| n.to_str()) {
        if skips.contains(dir_name) {
            return Ok(());
        }
    }

    detect_and_clean(&path, del_mode, children)
        .with_context(|| format!("cleaning directory {:?}", path.display()))?;

    let rd = read_dir(&path).with_context(|| format!("reading directory {:?}", path.display()))?;

    for entry in rd {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Err(e) = process_dir(entry.path(), depth - 1, skips, del_mode, children) {
                eprintln!("{:#}", e);
            }
        }
    }

    Ok(())
}

fn detect_and_clean(path: &Path, del_mode: DeleteMode, children: &mut Vec<Child>) -> Result<()> {
    let is_cargo_dir = path.join("Cargo.toml").is_file();
    if !is_cargo_dir {
        return Ok(());
    }

    eprintln!("Checking {:?}", path);

    if del_mode.do_all() {
        children.push(spawn_cargo_clean(path, &[])?);
    }
    if del_mode.do_release() {
        children.push(spawn_cargo_clean(path, &["--release"])?);
    }
    if del_mode.do_doc() {
        children.push(spawn_cargo_clean(path, &["--doc"])?);
    }

    Ok(())
}

fn spawn_cargo_clean(current_dir: &Path, args: &[&str]) -> Result<Child> {
    Command::new("cargo")
        .arg("clean")
        .args(args)
        .current_dir(current_dir)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .spawn()
        .context("failed to spawn `cargo clean`")
}

#[derive(Debug, Clone, Copy)]
struct DeleteMode {
    doc: bool,
    release: bool,
}

impl DeleteMode {
    fn do_all(self) -> bool {
        !self.release && !self.doc
    }

    fn do_doc(self) -> bool {
        self.doc
    }

    fn do_release(self) -> bool {
        self.release
    }
}
