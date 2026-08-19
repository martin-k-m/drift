# drift

[![CI](https://github.com/martin-k-m/drift/actions/workflows/ci.yml/badge.svg)](https://github.com/martin-k-m/drift/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/rustc-1.74+-blue.svg)](https://blog.rust-lang.org/2023/11/16/Rust-1.74.0.html)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Diff two tabular datasets. Schema changes, added and removed rows, and which
fields moved. Zero dependencies.

Not on crates.io yet. Install from the repository:

```bash
cargo install --git https://github.com/martin-k-m/drift
```

The registry name will be `drift-tabular`, because `drift` was taken. The
command stays `drift` either way; only the package name differs.

```bash
drift before.csv after.csv --key id
drift before.csv after.csv --key region,id --ignore updated_at
```

```
schema
  + column region

changed · 2
  1041
    status: pending → shipped
  1077
    total: 19.99 → 24.99

removed · 1
  − 1002

312 unchanged · 2 changed · 0 added · 1 removed
```

## Keyed, not positional

`diff` on two exports reports every row after the first insertion as changed.
That is true of the text and useless about the data.

`drift` matches rows on a key column, so a row that only moved is unchanged and
only a row whose fields actually differ is reported. Sorting a file, appending
to it, or re-exporting in a different order produces no diff at all.

## Composite keys

`--key region,id` when no single column identifies a row. The parts are kept
separate rather than joined into one string, because any separator can appear
in the data: joined on a comma, `("a,b", "c")` and `("a", "b,c")` become the
same key and two unrelated rows get paired.

## Columns that always move

An `updated_at` that changes on every export makes every row a change and
buries the fields anyone cares about. `--ignore updated_at,exported_by` leaves
those columns out of the comparison entirely, not compared and forgiven, but
never compared, so they cannot make a row count as changed.

A column named as both a key and ignored is an error. A key is never compared
as a field anyway, so the command is not wrong so much as not what its author
meant.

## Float noise

A pipeline re-run that rewrites `0.1000001` as `0.1000002` changes every row and
means nothing. `--epsilon` compares numerically where both sides are numbers:

```bash
drift before.csv after.csv --key id --epsilon 1e-6
```

Text is still compared exactly. A value that does not parse as a number is never
waved through because the comparison could not be made, `one` against `two`
stays a change no matter how large the tolerance. Two differently-spelled NaNs
stay a change for the same reason.

## What it will not do

**Report a schema change twice.** A column that exists on one side only is a
schema change, printed once. It is excluded from the field comparison, so it
does not also appear as a change on every single row.

**Silently pick a winner for duplicate keys.** With a repeated key there is no
single "before" row to compare against, so drift warns about every key it saw
more than once, on either side, and the warning survives even when nothing else
moved.

It still compares the key, against the last occurrence in the file. That means
the answer depends on the order the rows appear in: the same two files, with two
duplicate rows swapped, report a different "after" value. The warning is what
makes that visible rather than silent, and it is the reason to fix the export
rather than read the diff. If your data can repeat a key and you need an answer
that does not move, key on enough columns to make rows unique.

**Truncate without saying so.** Long lists stop at 20 entries and print how many
more there are. A diff that hides rows silently reads as a complete answer and
is not one.

**Print in a different order each run.** Everything is sorted before output, so
two runs over the same input are byte-identical and the result can be committed
as a fixture.

## Other delimiters

Comma is the default. `--delimiter` (or `-d`) takes a single character for
files separated by something else, and the word `tab` (also `\t`) for a tab,
since a literal tab is awkward to pass at a shell:

```bash
drift before.tsv after.tsv --key id --delimiter tab
drift before.txt after.txt --key id -d ';'
```

The quoting rules are unchanged with any delimiter, so a tab or a semicolon
inside a quoted field is data, not a field break. Both files are read with the
same delimiter. A multi-character delimiter is rejected rather than having its
first character used, which would silently ignore the rest.

## Summary

`--summary` prints only the counts, columns added and removed, rows added,
removed, and changed, and a duplicate-key warning if there is one, and none of
the per-field detail. It is for scripting and for files large enough that the
full listing is noise. The exit codes are unchanged, so it is a narrower view of
the same answer:

```
columns · 1 added · 0 removed
rows · 1 added · 1 removed · 1 changed · 0 unchanged
```

With `--json` it emits just the counts object rather than the full report.

## JSON output

```sh
drift before.csv after.csv --key id --json
```

The exit code answers "did the data move" and nothing else, so a pipeline that
wants to know *what* moved had to parse output written for a person, which is
output that is allowed to change. `--json` writes the whole report instead.

Keys are arrays rather than the joined display form, because a consumer that
has to split `"a . b"` back apart is one whose data cannot contain the
separator. There is an `identical` field so a caller can branch without
re-deriving it from four empty arrays and getting the duplicate-key case wrong.

`--json` wins over `--quiet`: asking for the report in a form a script can read
is a statement about what you want, and silence is not it.

Written by hand, like the CSV reader, so the dependency count stays at zero.
Escaping is the whole job: cell values are arbitrary bytes from somebody's
export, and control characters that are legal in a CSV field are illegal raw in
a JSON string.

## Exit codes

`0` identical · `1` differences found · `2` could not run

The exit code is the point for scripting, a pipeline can ask "did this dataset
move" without parsing anything:

```bash
drift yesterday.csv today.csv --key id --quiet || echo "data changed"
```

## Zero dependencies

A tool that reads two files and compares them should not pull in a tree you
then have to audit. The CSV reader is about a hundred lines and covers RFC 4180
as it actually appears: quoted fields, doubled quotes, embedded commas and
newlines, `\r\n` and `\n`, ragged rows, a leading byte-order mark, and files
with no trailing newline.

Not covered, on purpose: comments and encoding detection. Those are the features
that turn a parser into a library, and this is not trying to be one.

Colour is disabled when `NO_COLOR` is set, and by `--no-color`.

## Related

Four small tools that each do one thing to a table of data:

- [csvpeek](https://github.com/martin-k-m/csvpeek) profiles a file: column
  types, null counts, distributions.
- [sift](https://github.com/martin-k-m/sift) queries one: filter, sort,
  aggregate, streaming.
- **drift** diffs two of them. The only one of the four in Rust.
- [quarry](https://github.com/martin-k-m/quarry) is the long way round, a
  hand-written SQL parser and executor meant to be read.

## License

MIT
