cargo-clean-recursive
=======================

A cargo subcommand cleans all projects under specified directory.

## Installation

Get package with Cargo:

```
cargo install cargo-clean-recursive
```

## Usage

To clean all projects under current directory, run this subcommand with no option:

```
cargo clean-recursive
```

If you want to clean releace build only, use `--release / -r` option:

```
cargo clean-recursive --release
```

Also, you can use `--doc / -d` option to clean docs:

```
cargo clean-recursive --doc
```

You can specify `--release` and `--doc` at the same time.


