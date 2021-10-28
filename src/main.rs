use std::env::{args, current_dir};
use std::path::{Path, PathBuf};

use async_recursion::async_recursion;
use tokio::fs::read_dir;
use tokio::process::Command;

use anyhow::{Context, Result};
use clap::{App, Arg};

#[tokio::main]
async fn main() -> Result<()> {
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
        .with_context(|| format!("parsing {} as number", depth_str))?;

    let path = if let Some(path) = matches.value_of("path") {
        PathBuf::from(path)
    } else {
        current_dir().context("getting current_dir")?
    };

    process_dir(Path::new(&path), depth, delete_mode).await?;

    Ok(())
}

#[async_recursion]
async fn process_dir(path: &Path, depth: usize, del_mode: DeleteMode) -> Result<()> {
    if depth == 0 {
        return Ok(());
    }

    detect_and_clean(path, del_mode)
        .await
        .with_context(|| format!("cleaning directory {:?}", path))?;

    let mut rd = read_dir(path)
        .await
        .with_context(|| format!("reading directory {:?}", path.canonicalize()))?;
    while let Some(e) = rd.next_entry().await? {
        if e.file_type().await?.is_dir() {
            if let Err(e) = process_dir(&e.path(), depth - 1, del_mode).await {
                eprintln!("Warn: {}", e);
                for c in e.chain().skip(1) {
                    eprintln!("    at: {}", c);
                }
            }
        }
    }

    Ok(())
}

async fn detect_and_clean(path: &Path, del_mode: DeleteMode) -> Result<()> {
    if !path.join("Cargo.toml").exists() {
        return Ok(());
    }

    let target_dir = path.join("target");
    if !target_dir.exists() || !target_dir.is_dir() {
        return Ok(());
    }

    eprintln!("Cleaning {:?}", path);

    if del_mode.do_all() {
        Command::new("cargo")
            .args(&["clean"])
            .current_dir(path)
            .output()
            .await?;
    }
    if del_mode.do_release() {
        Command::new("cargo")
            .args(&["clean", "--release"])
            .current_dir(path)
            .output()
            .await?;
    }
    if del_mode.do_doc() {
        Command::new("cargo")
            .args(&["clean", "--doc"])
            .current_dir(path)
            .output()
            .await?;
    }

    Ok(())
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
