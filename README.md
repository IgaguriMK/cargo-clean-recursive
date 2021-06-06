cargo-clean-recursive
=======================

A cargo subcommand cleans all projects under a specified directory.

## Installation

Build binary with Cargo:

```
cargo install cargo-clean-recursive
```

## Usage

To clean all projects under current directory, run this subcommand with no option:

```
cargo clean-recursive
```

If you want to clean release build only, use `--release / -r` flag:

```
cargo clean-recursive --release
```

Also, you can use `--doc / -d` option to clean docs:

```
cargo clean-recursive --doc
```

You can specify `--release` and `--doc` at the same time.


