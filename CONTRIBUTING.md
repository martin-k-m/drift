# Contributing to drift

Thanks for taking a look. `drift` is small and dependency-free on purpose, and
the bar for a change is that it stays that way.

## Setup

A stable Rust toolchain is all you need.

```bash
git clone https://github.com/martin-k-m/drift
cd drift
cargo test --all
```

## Ground rules

- **Zero dependencies.** The `[dependencies]` table is deliberately empty. A
  change that adds a crate needs to justify why the standard library genuinely
  cannot do it. The CSV reader is about a hundred lines and that is the point.
- **The CSV reader covers RFC 4180 as it appears in the wild**: quoted fields,
  doubled quotes, embedded commas and newlines, `\r\n` and `\n`, ragged rows, a
  leading BOM, and no trailing newline. A change to parsing comes with a test
  that pins the case it handles.
- **Keyed comparison, not positional.** The reason `drift` exists is that a row
  which only moved is unchanged. Keep that invariant.
- **Colour respects `NO_COLOR` and `--no-color`.** Any new output honours both.

## Before you open a pull request

The CI gates on all three; run them locally first:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

CI runs this matrix on Linux, macOS, and Windows. Windows especially, because
the `\r\n` handling in the CSV reader is code a Linux-only run never exercises.

A separate job builds and tests on the minimum supported Rust version, 1.74,
which is the `rust-version` in `Cargo.toml` and the number on the README badge.
A change that needs a newer standard library method has to bump all three.

## Reporting bugs

Open an issue with the two input files (a few rows each is plenty), the exact
command, and the diff you expected versus what you got. A failing case added to
`tests/cli.rs` is the most useful report there is.
