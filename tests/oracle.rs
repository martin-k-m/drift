//! Property test: drift's classification against an independent reference.
//!
//! Every other test here is an example someone wrote down, which can only find
//! the mistakes that person thought of. This generates random pairs of tables,
//! classifies every key a second time with a reference diff written in this
//! file, and compares the counts. The reference is deliberately naive: two maps
//! and a field-by-field walk, no shared code with `src/diff.rs`, so agreement
//! means two implementations reached the same answer rather than one
//! implementation agreeing with itself.
//!
//! Keys are unique on both sides here. A repeated key is compared against its
//! last occurrence, which makes the answer depend on the order rows appear in
//! the file, and a reference that had to reproduce that would be encoding the
//! behaviour rather than checking it.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_drift");
const TRIALS: u64 = 300;
const SEED: u64 = 0x5eed_1234;

/// xorshift64*, so the cases are reproducible without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

const VALUES: [&str; 4] = ["a", "b", "", "19.99"];

/// One table: the column names and the rows, each row keyed by `id`.
struct Table {
    columns: Vec<String>,
    rows: BTreeMap<String, Vec<String>>,
}

impl Table {
    fn to_csv(&self) -> String {
        let mut out = self.columns.join(",");
        out.push('\n');
        for (id, values) in &self.rows {
            out.push_str(id);
            for v in values {
                out.push(',');
                out.push_str(v);
            }
            out.push('\n');
        }
        out
    }
}

/// A table whose first column is `id` and whose others are drawn from VALUES.
///
/// `carry` is the previous table, if any. Rows are copied from it unchanged
/// about half the time, because two independently random tables almost never
/// agree on a row and the unchanged path would go untested: an early version of
/// this generator produced 7 unchanged rows across 300 trials.
fn generate(rng: &mut Rng, columns: &[String], carry: Option<&Table>) -> Table {
    let mut rows = BTreeMap::new();
    let count = rng.below(9);
    for _ in 0..count {
        let id = (rng.below(12) + 1).to_string();
        let mut values: Vec<String> = columns[1..]
            .iter()
            .map(|_| VALUES[rng.below(VALUES.len())].to_string())
            .collect();
        if let Some(previous) = carry {
            if rng.chance(50) {
                if let Some(old) = previous.rows.get(&id) {
                    for (i, column) in columns[1..].iter().enumerate() {
                        if let Some(j) = previous.columns.iter().position(|c| c == column) {
                            values[i] = old[j - 1].clone();
                        }
                    }
                }
            }
        }
        rows.insert(id, values);
    }
    Table {
        columns: columns.to_vec(),
        rows,
    }
}

struct Counts {
    columns_added: usize,
    columns_removed: usize,
    rows_added: usize,
    rows_removed: usize,
    rows_changed: usize,
    unchanged: usize,
}

/// The reference. Two maps and a walk, which is the whole definition of a
/// keyed diff and shares nothing with the implementation under test.
fn reference(left: &Table, right: &Table, ignore: Option<&str>) -> Counts {
    let added_columns = right
        .columns
        .iter()
        .filter(|c| !left.columns.contains(c))
        .count();
    let removed_columns = left
        .columns
        .iter()
        .filter(|c| !right.columns.contains(c))
        .count();

    let comparable: Vec<&String> = left.columns[1..]
        .iter()
        .filter(|c| right.columns.contains(c) && Some(c.as_str()) != ignore)
        .collect();

    let value = |t: &Table, id: &str, column: &String| -> Option<String> {
        let i = t.columns.iter().position(|c| c == column)?;
        t.rows.get(id).map(|r| r[i - 1].clone())
    };

    let mut rows_changed = 0;
    let mut unchanged = 0;
    for id in left.rows.keys() {
        if !right.rows.contains_key(id) {
            continue;
        }
        let moved = comparable
            .iter()
            .any(|c| value(left, id, c) != value(right, id, c));
        if moved {
            rows_changed += 1;
        } else {
            unchanged += 1;
        }
    }

    Counts {
        columns_added: added_columns,
        columns_removed: removed_columns,
        rows_added: right
            .rows
            .keys()
            .filter(|k| !left.rows.contains_key(*k))
            .count(),
        rows_removed: left
            .rows
            .keys()
            .filter(|k| !right.rows.contains_key(*k))
            .count(),
        rows_changed,
        unchanged,
    }
}

/// Pull `"name": <integer>` out of the summary JSON. Hand-written because the
/// crate has no dependencies and this needs six integers, not a parser.
fn number(json: &str, name: &str) -> usize {
    let needle = format!("\"{name}\":");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("no {name} in {json}"))
        + needle.len();
    json[start..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a number in {json}"))
}

#[test]
fn drift_classifies_every_row_the_way_a_reference_diff_does() {
    let dir = std::env::temp_dir().join(format!("drift-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut rng = Rng(SEED);
    let mut compared = 0;

    for trial in 0..TRIALS {
        let mut left_columns: Vec<String> = ["id", "region", "status", "total"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        if rng.chance(30) {
            left_columns.push("note".to_string());
        }
        let mut right_columns = left_columns.clone();
        if rng.chance(20) {
            right_columns.push("extra".to_string());
        }
        if rng.chance(20) && right_columns.len() > 2 {
            right_columns.pop();
        }
        let ignore = if rng.chance(25) { Some("status") } else { None };

        let left = generate(&mut rng, &left_columns, None);
        let right = generate(&mut rng, &right_columns, Some(&left));

        let lp: PathBuf = dir.join(format!("l{trial}.csv"));
        let rp: PathBuf = dir.join(format!("r{trial}.csv"));
        fs::write(&lp, left.to_csv()).unwrap();
        fs::write(&rp, right.to_csv()).unwrap();

        let mut args: Vec<String> = vec![
            lp.to_str().unwrap().to_string(),
            rp.to_str().unwrap().to_string(),
            "--key".into(),
            "id".into(),
            "--summary".into(),
            "--json".into(),
        ];
        if let Some(column) = ignore {
            args.push("--ignore".into());
            args.push(column.into());
        }
        let out = Command::new(BIN).args(&args).output().unwrap();
        let json = String::from_utf8(out.stdout).unwrap();

        let want = reference(&left, &right, ignore);
        let context = format!(
            "trial {trial}\nleft:\n{}\nright:\n{}\ndrift: {json}",
            left.to_csv(),
            right.to_csv()
        );
        assert_eq!(
            number(&json, "columnsAdded"),
            want.columns_added,
            "{context}"
        );
        assert_eq!(
            number(&json, "columnsRemoved"),
            want.columns_removed,
            "{context}"
        );
        assert_eq!(number(&json, "rowsAdded"), want.rows_added, "{context}");
        assert_eq!(number(&json, "rowsRemoved"), want.rows_removed, "{context}");
        assert_eq!(number(&json, "rowsChanged"), want.rows_changed, "{context}");
        assert_eq!(number(&json, "unchanged"), want.unchanged, "{context}");
        compared += 1;
    }

    let _ = fs::remove_dir_all(&dir);
    // A generator that stopped producing cases would leave every assertion
    // above unreached and this test still green.
    assert_eq!(compared, TRIALS);
}
