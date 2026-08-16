# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

0.1.0 was never tagged or released; it describes the work rather than a
downloadable artefact. 0.2.0 is the first release.

## 0.2.0 - 2026-08-16

The first published version. It goes to crates.io as `drift-tabular`, because
`drift` was taken and so was the `drift-diff` this package used to declare: an
unrelated crate claimed it in August 2026, which meant `cargo install
drift-diff` fetched a different tool. The binary, the repository and every
command a user types stay `drift`.

### Added
- Composite keys (`--key region,id`) and ignored columns (`--ignore updated_at`).
- Numeric comparison with a tolerance, so a negligible float difference is not
  reported as a change.
- JSON report output, for piping the diff into other tools.
- A configurable delimiter and a summary-only mode.
- A quiet mode (`--quiet`) that sets the exit code without printing, for use in
  scripts (`drift a.csv b.csv --key id --quiet || echo changed`).

### Fixed
- A duplicate key in the input is no longer silently hidden; it is warned about.
- A leading byte-order mark is stripped rather than becoming part of the first
  column name.
- README, SECURITY and CHANGELOG no longer point at a crates.io package and
  release tags that did not exist.

### Also in this release
- Contribution, security and changelog documentation, and status badges in the
  README.
- A CI job that builds and tests on the declared minimum supported Rust version.

## 0.1.0

### Added
- Initial version: keyed diff of two tabular files reporting schema changes,
  added and removed rows, and per-field changes, with a hand-written
  RFC 4180 CSV reader and zero dependencies. MIT licensed.
