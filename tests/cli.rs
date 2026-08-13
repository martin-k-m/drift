//! End-to-end tests that run the compiled `drift` binary.
//!
//! The unit tests cover the diff and CSV logic directly. These cover the parts
//! only the binary exercises: argument parsing, the three exit codes, and the
//! human-readable output, including the duplicate-key warning that must survive
//! even when nothing else moved.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_drift");

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A scratch directory unique to one test, removed when it drops.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new() -> Scratch {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("drift-it-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        Scratch { dir }
    }

    /// Write a file and return its path as a string.
    fn write(&self, name: &str, contents: &str) -> String {
        let p = self.dir.join(name);
        fs::write(&p, contents).unwrap();
        p.to_str().unwrap().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Run the binary and return (exit code, stdout, stderr).
fn run(args: &[&str]) -> (i32, String, String) {
    // NO_COLOR keeps stdout plain so assertions can match on text.
    let out = Command::new(BIN)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run drift");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn identical_files_exit_zero() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no differences"), "{out}");
}

#[test]
fn a_difference_exits_one() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,z\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("changed"), "{out}");
    assert!(out.contains("a → z"), "{out}");
}

#[test]
fn a_missing_file_exits_two() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let (code, _, err) = run(&[&a, "does-not-exist.csv", "--key", "id"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("drift:"), "{err}");
}

#[test]
fn a_missing_key_flag_exits_two() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, _, err) = run(&[&a, &b]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--key is required"), "{err}");
}

#[test]
fn a_key_that_is_also_ignored_is_rejected() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, _, err) = run(&[&a, &b, "--key", "id", "--ignore", "id"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("both a key and ignored"), "{err}");
}

#[test]
fn an_unknown_option_exits_two() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, _, err) = run(&[&a, &b, "--key", "id", "--bogus"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("unknown option"), "{err}");
}

#[test]
fn quiet_prints_nothing_but_keeps_the_exit_code() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,z\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--quiet"]);
    assert_eq!(code, 1);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn json_wins_over_quiet() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,z\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--quiet", "--json"]);
    assert_eq!(code, 1);
    assert!(out.contains("\"identical\": false"), "{out}");
    assert!(out.trim_end().ends_with('}'), "{out}");
}

#[test]
fn duplicate_keys_are_warned_about_even_when_nothing_else_moved() {
    // Regression: the human printer returned early on "no differences" and hid
    // this warning. JSON always carried it; the person-facing output must too.
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n1,a\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id"]);
    // The data is identical, so the exit code stays 0.
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("duplicate key"), "{out}");
}

#[test]
fn a_composite_key_matches_on_both_parts() {
    let s = Scratch::new();
    let a = s.write("a.csv", "region,id,v\nus,1,a\neu,1,b\n");
    let b = s.write("b.csv", "region,id,v\nus,1,a\neu,1,Z\n");
    let (code, out, _) = run(&[&a, &b, "--key", "region,id"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("eu · 1"), "{out}");
}

#[test]
fn ignore_drops_a_column_from_the_comparison() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v,updated\n1,a,09:00\n");
    let b = s.write("b.csv", "id,v,updated\n1,a,17:00\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--ignore", "updated"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn epsilon_absorbs_float_noise_from_the_command_line() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,0.1000001\n");
    let b = s.write("b.csv", "id,v\n1,0.1000002\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--epsilon", "1e-6"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn a_bad_epsilon_exits_two() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, _, err) = run(&[&a, &b, "--key", "id", "--epsilon", "-1"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("epsilon"), "{err}");
}

#[test]
fn help_and_version_exit_zero() {
    let (hc, hout, _) = run(&["--help"]);
    assert_eq!(hc, 0);
    assert!(hout.contains("usage"), "{hout}");

    let (vc, vout, _) = run(&["--version"]);
    assert_eq!(vc, 0);
    assert!(vout.contains("drift"), "{vout}");
}

#[test]
fn a_tab_delimited_file_diffs_with_delimiter_tab() {
    let s = Scratch::new();
    let a = s.write("a.tsv", "id\tv\n1\ta\n");
    let b = s.write("b.tsv", "id\tv\n1\tz\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--delimiter", "tab"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("a → z"), "{out}");
}

#[test]
fn a_quoted_tab_in_a_tsv_field_is_not_a_column_break() {
    // The value has a tab inside quotes, so the two files match on their single
    // data column and nothing moved.
    let s = Scratch::new();
    let a = s.write("a.tsv", "id\tv\n1\t\"x\ty\"\n");
    let b = s.write("b.tsv", "id\tv\n1\t\"x\ty\"\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "-d", "tab"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no differences"), "{out}");
}

#[test]
fn a_semicolon_delimiter_is_accepted() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id;v\n1;a\n");
    let b = s.write("b.csv", "id;v\n1;z\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "-d", ";"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("a → z"), "{out}");
}

#[test]
fn a_multi_character_delimiter_is_rejected() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, _, err) = run(&[&a, &b, "--key", "id", "--delimiter", "::"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("single character"), "{err}");
}

#[test]
fn summary_prints_counts_not_per_field_detail() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n2,b\n");
    let b = s.write("b.csv", "id,v\n1,z\n3,c\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--summary"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("rows · 1 added · 1 removed · 1 changed"),
        "{out}"
    );
    // The per-field detail is not printed in summary mode.
    assert!(!out.contains("a → z"), "{out}");
}

#[test]
fn summary_of_identical_files_exits_zero() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--summary"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0 changed · 1 unchanged"), "{out}");
}

#[test]
fn summary_reports_duplicate_key_counts() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n1,a\n");
    let b = s.write("b.csv", "id,v\n1,a\n1,a\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--summary"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("duplicate key"), "{out}");
}

#[test]
fn summary_json_emits_just_the_counts_object() {
    let s = Scratch::new();
    let a = s.write("a.csv", "id,v\n1,a\n2,b\n");
    let b = s.write("b.csv", "id,v\n1,z\n3,c\n");
    let (code, out, _) = run(&[&a, &b, "--key", "id", "--summary", "--json"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("\"rowsChanged\": 1"), "{out}");
    assert!(out.contains("\"rowsAdded\": 1"), "{out}");
    assert!(out.contains("\"rowsRemoved\": 1"), "{out}");
    // The full report's per-row detail is absent.
    assert!(!out.contains("\"before\""), "{out}");
    assert!(out.trim_end().ends_with('}'), "{out}");
}

#[test]
fn a_bom_in_the_export_does_not_hide_the_key_column() {
    // Excel writes a UTF-8 BOM; without stripping it, --key id fails over a file
    // that plainly has an id column.
    let s = Scratch::new();
    let a = s.write("a.csv", "\u{feff}id,v\n1,a\n");
    let b = s.write("b.csv", "\u{feff}id,v\n1,z\n");
    let (code, out, err) = run(&[&a, &b, "--key", "id"]);
    assert_eq!(code, 1, "out={out} err={err}");
    assert!(out.contains("a → z"), "{out}");
}
