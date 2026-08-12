# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Contribution, security, and changelog documentation, and status badges in the
  README.

## [0.2.0]

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

## [0.1.0]

### Added
- Initial release: keyed diff of two tabular files reporting schema changes,
  added and removed rows, and per-field changes, with a hand-written
  RFC 4180 CSV reader and zero dependencies. MIT licensed.

[Unreleased]: https://github.com/martin-k-m/drift/compare/main...HEAD
[0.2.0]: https://github.com/martin-k-m/drift/releases/tag/v0.2.0
[0.1.0]: https://github.com/martin-k-m/drift/releases/tag/v0.1.0
