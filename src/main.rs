use std::env::{args, current_dir};
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command};

use anyhow::{Context, Result};
use clap::{App, Arg};

fn main() -> Result<()> {
    let mut args: Vec<String> = args().collect();
    if args.len() >= 2 && &args[1] == "clean-recursive" {
        args.remove(1);
    }

    let matches = App::new("cargo clean-recursive")
        .bin_name("cargo clean-recursive")
        .arg(
            Arg::with_name("doc")
                .short("d")
                .long("doc")
                .help("Deletes documents"),
        )
        .arg(
            Arg::with_name("release")
                .short("r")
                .long("release")
                .help("Deletes release target"),
        )
        .arg(
            Arg::with_name("depth")
                .long("depth")
                .default_value("64")
                .help("Recursive serarch depth limit"),
        )
        .arg(
            Arg::with_name("path")
                .short("p")
                .long("path")
                .help("Target directory"),
        )
        .get_matches_from(&args);

    let delete_mode = DeleteMode {
        doc: matches.is_present("doc"),
        release: matches.is_present("release"),
    };

    let depth_str = matches.value_of("depth").expect("'depth' should be exists");
    let depth: usize = depth_str
        .parse()
        .with_context(|| format!("parsing '{}' as number", depth_str))?;

    let path = if let Some(path) = matches.value_of("path") {
        PathBuf::from(path)
    } else {
        current_dir().context("getting current_dir")?
    };

    let mut children = Vec::new();

    process_dir(path, depth, delete_mode, &mut children)?;

    for mut child in children {
        if let Err(e) = child.wait() {
            eprintln!("{:#}", e);
        }
    }

    Ok(())
}

fn process_dir(
    path: PathBuf,
    depth: usize,
    del_mode: DeleteMode,
    children: &mut Vec<Child>,
) -> Result<()> {
    if depth == 0 {
        return Ok(());
    }

    detect_and_clean(&path, del_mode, children)
        .with_context(|| format!("cleaning directory {:?}", path))?;

    let rd =
        read_dir(&path).with_context(|| format!("reading directory {:?}", path.canonicalize()))?;

    for entry in rd {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Err(e) = process_dir(entry.path(), depth - 1, del_mode, children) {
                eprintln!("{:#}", e);
            }
        }
    }

    Ok(())
}

fn detect_and_clean(path: &Path, del_mode: DeleteMode, children: &mut Vec<Child>) -> Result<()> {
    let should_clean = path.join("Cargo.toml").is_file() && path.join("target").is_dir();
    if !should_clean {
        return Ok(());
    }

    eprintln!("Cleaning {:?}", path);

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
