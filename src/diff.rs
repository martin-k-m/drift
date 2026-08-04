//! Comparing two tables.
//!
//! The comparison is keyed, not positional. Diffing two exports line by line
//! reports every row after the first insertion as changed, which is true of the
//! text and useless about the data. Matching on a key column instead means a
//! row that moved is unchanged, and only a row whose fields differ is reported
//! as changed.

use crate::csv::Table;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Report {
    pub added_columns: Vec<String>,
    pub removed_columns: Vec<String>,
    pub reordered: bool,
    pub added_rows: Vec<Key>,
    pub removed_rows: Vec<Key>,
    pub changed: Vec<Change>,
    pub duplicate_keys: Vec<Key>,
    pub unchanged: usize,
}

#[derive(Debug)]
pub struct Change {
    pub key: Key,
    /// Column, before, after.
    pub fields: Vec<(String, String, String)>,
}

impl Report {
    /// Whether anything at all differs. Drives the exit code, so a script can
    /// branch on "did this dataset move" without parsing the output.
    pub fn any(&self) -> bool {
        !self.added_columns.is_empty()
            || !self.removed_columns.is_empty()
            || self.reordered
            || !self.added_rows.is_empty()
            || !self.removed_rows.is_empty()
            || !self.changed.is_empty()
    }
}

/// A row's identity: one field per key column, in the order they were named.
///
/// Held as a vector rather than a joined string on purpose. Joining on a
/// separator makes ("a,b", "c") and ("a", "b,c") the same key, which would
/// silently pair two unrelated rows — and any separator can appear in data, so
/// there is no character that makes the problem go away. The vector is only
/// flattened for display, where a collision costs nothing.
type Key = Vec<String>;

/// Render a key for output. Single keys print bare, so the common case reads
/// exactly as it did before composite keys existed.
pub fn show_key(k: &[String]) -> String {
    k.join(" · ")
}

/// Index a table by its key columns, remembering keys that appear twice.
///
/// Duplicates are reported rather than silently resolved: with a repeated key
/// there is no single "before" row to compare against, and picking one quietly
/// would make the diff depend on file order.
fn index<'a>(t: &'a Table, keys: &[usize]) -> (HashMap<Key, &'a Vec<String>>, Vec<Key>) {
    let mut map = HashMap::new();
    let mut dupes = Vec::new();
    for row in &t.rows {
        let k: Key = keys.iter().map(|&i| t.field(row, i).to_string()).collect();
        if map.insert(k.clone(), row).is_some() && !dupes.contains(&k) {
            dupes.push(k);
        }
    }
    (map, dupes)
}

/// How two field values are judged equal.
#[derive(Clone, Copy)]
pub enum Tolerance {
    /// Byte equality. "1.0" and "1.00" are different.
    Exact,
    /// Numeric where both sides parse as numbers, byte equality otherwise.
    Absolute(f64),
}

impl Tolerance {
    fn same(self, a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        match self {
            Tolerance::Exact => false,
            Tolerance::Absolute(eps) => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                // Only when *both* sides are numbers. A column holding "1" and
                // "one" must not be quietly declared equal because one side
                // failed to parse.
                (Ok(x), Ok(y)) => {
                    // NaN never equals anything, including itself, and a diff
                    // that reported two NaNs as unchanged would be hiding the
                    // fact that neither is a number any more.
                    if x.is_nan() || y.is_nan() {
                        false
                    } else {
                        (x - y).abs() <= eps
                    }
                }
                _ => false,
            },
        }
    }
}

/// Resolve key column names to indices in one table, naming the file in the
/// error so a typo says which side it could not be found on.
fn key_indices(t: &Table, keys: &[String], which: &str) -> Result<Vec<usize>, String> {
    keys.iter()
        .map(|k| {
            t.column(k).ok_or_else(|| {
                format!(
                    "no column {k:?} in the {which} file; it has {}",
                    list(&t.header)
                )
            })
        })
        .collect()
}

pub fn compare(
    before: &Table,
    after: &Table,
    keys: &[String],
    ignore: &[String],
    tol: Tolerance,
) -> Result<Report, String> {
    if keys.is_empty() {
        return Err("at least one key column is needed".into());
    }
    let bk = key_indices(before, keys, "first")?;
    let ak = key_indices(after, keys, "second")?;

    let added_columns: Vec<String> = after
        .header
        .iter()
        .filter(|c| !before.header.contains(c))
        .cloned()
        .collect();
    let removed_columns: Vec<String> = before
        .header
        .iter()
        .filter(|c| !after.header.contains(c))
        .cloned()
        .collect();

    // Only meaningful when the sets match; otherwise added/removed says it.
    let reordered =
        added_columns.is_empty() && removed_columns.is_empty() && before.header != after.header;

    let (bmap, mut dupes) = index(before, &bk);
    let (amap, adupes) = index(after, &ak);
    for d in adupes {
        if !dupes.contains(&d) {
            dupes.push(d);
        }
    }

    // Compare only the columns both files have; a column that exists on one
    // side is a schema change, already reported, and would otherwise show up
    // again as a change on every single row.
    //
    // Ignored columns drop out here too. The usual reason is a timestamp that
    // moves on every export and drowns the fields anyone cares about, and the
    // point of dropping it at this step is that an ignored column cannot make
    // a row count as changed — it is not compared at all rather than compared
    // and forgiven.
    let shared: Vec<&String> = before
        .header
        .iter()
        .filter(|c| after.header.contains(c))
        .filter(|c| !ignore.contains(c))
        .collect();

    let mut added_rows = Vec::new();
    let mut removed_rows = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0usize;

    for (k, brow) in &bmap {
        match amap.get(k) {
            None => removed_rows.push(k.clone()),
            Some(arow) => {
                let mut fields = Vec::new();
                for col in &shared {
                    let b = before.field(brow, before.column(col).unwrap());
                    let a = after.field(arow, after.column(col).unwrap());
                    if !tol.same(b, a) {
                        fields.push(((*col).clone(), b.to_string(), a.to_string()));
                    }
                }
                if fields.is_empty() {
                    unchanged += 1;
                } else {
                    changed.push(Change {
                        key: k.clone(),
                        fields,
                    });
                }
            }
        }
    }

    for k in amap.keys() {
        if !bmap.contains_key(k) {
            added_rows.push(k.clone());
        }
    }

    // HashMap iteration order is deliberately unspecified, so sort everything:
    // a diff that prints its findings in a different order each run cannot be
    // compared between runs or committed to a fixture.
    added_rows.sort();
    removed_rows.sort();
    dupes.sort();
    changed.sort_by(|a, b| a.key.cmp(&b.key));

    Ok(Report {
        added_columns,
        removed_columns,
        reordered,
        added_rows,
        removed_rows,
        changed,
        duplicate_keys: dupes,
        unchanged,
    })
}

fn list(cols: &[String]) -> String {
    if cols.is_empty() {
        "none".into()
    } else {
        cols.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::parse;

    /// Keys as display strings, so an assertion reads as the output does
    /// rather than as a vector of vectors.
    fn shown(keys: &[Key]) -> Vec<String> {
        keys.iter().map(|k| show_key(k)).collect()
    }

    fn cmp(a: &str, b: &str) -> Report {
        compare(
            &parse(a).unwrap(),
            &parse(b).unwrap(),
            &["id".into()],
            &[],
            Tolerance::Exact,
        )
        .unwrap()
    }

    fn cmp_eps(a: &str, b: &str, eps: f64) -> Report {
        compare(
            &parse(a).unwrap(),
            &parse(b).unwrap(),
            &["id".into()],
            &[],
            Tolerance::Absolute(eps),
        )
        .unwrap()
    }

    #[test]
    fn identical_files_report_nothing() {
        let r = cmp("id,v\n1,a\n", "id,v\n1,a\n");
        assert!(!r.any());
        assert_eq!(r.unchanged, 1);
    }

    #[test]
    fn a_moved_row_is_not_a_change() {
        // The whole reason for keying rather than diffing line by line.
        let r = cmp("id,v\n1,a\n2,b\n", "id,v\n2,b\n1,a\n");
        assert!(!r.any());
        assert_eq!(r.unchanged, 2);
    }

    #[test]
    fn changed_field_names_the_column_and_both_values() {
        let r = cmp("id,v\n1,a\n", "id,v\n1,z\n");
        assert_eq!(r.changed.len(), 1);
        assert_eq!(r.changed[0].fields[0], ("v".into(), "a".into(), "z".into()));
    }

    #[test]
    fn added_and_removed_rows() {
        let r = cmp("id,v\n1,a\n2,b\n", "id,v\n2,b\n3,c\n");
        assert_eq!(shown(&r.removed_rows), ["1"]);
        assert_eq!(shown(&r.added_rows), ["3"]);
    }

    #[test]
    fn schema_changes_are_separate_from_row_changes() {
        let r = cmp("id,v\n1,a\n", "id,v,extra\n1,a,x\n");
        assert_eq!(r.added_columns, ["extra"]);
        // The new column must not also be reported as a change on every row.
        assert!(r.changed.is_empty());
        assert_eq!(r.unchanged, 1);
    }

    #[test]
    fn removed_column_is_reported_once_not_per_row() {
        let r = cmp("id,v,gone\n1,a,x\n2,b,y\n", "id,v\n1,a\n2,b\n");
        assert_eq!(r.removed_columns, ["gone"]);
        assert!(r.changed.is_empty());
    }

    #[test]
    fn reordering_columns_is_flagged_but_is_not_a_row_change() {
        let r = cmp("id,a,b\n1,x,y\n", "id,b,a\n1,y,x\n");
        assert!(r.reordered);
        assert!(r.changed.is_empty());
    }

    #[test]
    fn duplicate_keys_are_reported() {
        let r = cmp("id,v\n1,a\n1,b\n", "id,v\n1,a\n");
        assert_eq!(shown(&r.duplicate_keys), ["1"]);
    }

    #[test]
    fn missing_key_column_names_what_is_there() {
        let e = compare(
            &parse("a\n1\n").unwrap(),
            &parse("a\n1\n").unwrap(),
            &["id".into()],
            &[],
            Tolerance::Exact,
        )
        .unwrap_err();
        assert!(e.contains("\"id\""), "{e}");
        assert!(e.contains("it has a"), "{e}");
    }

    #[test]
    fn tolerance_absorbs_float_noise() {
        // The case this exists for: a pipeline re-run that changes the last
        // digit of every float and nothing else.
        let r = cmp_eps(
            "id,v
1,0.1000001
",
            "id,v
1,0.1000002
",
            1e-6,
        );
        assert!(!r.any());
        assert_eq!(r.unchanged, 1);
    }

    #[test]
    fn tolerance_still_reports_a_real_move() {
        let r = cmp_eps(
            "id,v
1,10.0
",
            "id,v
1,10.5
",
            1e-6,
        );
        assert_eq!(r.changed.len(), 1);
    }

    #[test]
    fn tolerance_does_not_apply_to_text() {
        // "one" does not parse, so this must stay a change rather than being
        // waved through because the comparison could not be made.
        let r = cmp_eps(
            "id,v
1,one
",
            "id,v
1,two
",
            1000.0,
        );
        assert_eq!(r.changed.len(), 1);
    }

    #[test]
    fn tolerance_treats_equal_text_as_equal() {
        let r = cmp_eps(
            "id,v
1,abc
",
            "id,v
1,abc
",
            1e-9,
        );
        assert!(!r.any());
    }

    #[test]
    fn nan_is_never_unchanged() {
        let r = cmp_eps(
            "id,v
1,NaN
",
            "id,v
1,NaN
",
            1e9,
        );
        // Byte-equal, so it is unchanged — the guard is that tolerance does not
        // *additionally* declare two different NaN spellings equal.
        assert!(!r.any());
        let r2 = cmp_eps(
            "id,v
1,NaN
",
            "id,v
1,nan
",
            1e9,
        );
        assert_eq!(r2.changed.len(), 1);
    }

    #[test]
    fn exact_mode_sees_formatting_changes() {
        let r = cmp(
            "id,v
1,1.0
",
            "id,v
1,1.00
",
        );
        assert_eq!(r.changed.len(), 1);
        // And tolerance is what makes them the same number.
        assert!(!cmp_eps(
            "id,v
1,1.0
",
            "id,v
1,1.00
",
            0.0
        )
        .any());
    }

    #[test]
    fn output_order_is_stable() {
        let r = cmp("id,v\n3,a\n1,a\n2,a\n", "id,v\n\n");
        assert_eq!(shown(&r.removed_rows), ["1", "2", "3"]);
    }

    fn cmp_keys(a: &str, b: &str, keys: &[&str], ignore: &[&str]) -> Report {
        compare(
            &parse(a).unwrap(),
            &parse(b).unwrap(),
            &keys.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
            &ignore.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
            Tolerance::Exact,
        )
        .unwrap()
    }

    #[test]
    fn composite_key_identifies_a_row_by_both_columns() {
        // Neither column alone is unique; together they are.
        let before = "region,id,v
us,1,a
eu,1,b
";
        let after = "region,id,v
us,1,a
eu,1,CHANGED
";
        let r = cmp_keys(before, after, &["region", "id"], &[]);
        assert_eq!(r.unchanged, 1);
        assert_eq!(
            shown(&r.changed.iter().map(|c| c.key.clone()).collect::<Vec<_>>()),
            ["eu · 1"]
        );
    }

    #[test]
    fn a_single_column_key_is_not_reported_as_a_duplicate_when_composite() {
        // Keyed on id alone these two rows collide; keyed on both they do not.
        let f = "region,id,v
us,1,a
eu,1,b
";
        assert!(cmp_keys(f, f, &["id"], &[]).duplicate_keys.len() == 1);
        assert!(cmp_keys(f, f, &["region", "id"], &[])
            .duplicate_keys
            .is_empty());
    }

    #[test]
    fn key_parts_are_not_joined_into_an_ambiguous_string() {
        // ("a,b", "c") and ("a", "b,c") would be the same key under any
        // separator-joining scheme, which would pair two unrelated rows.
        let before = "x,y,v
\"a,b\",c,1
a,\"b,c\",2
";
        let after = before;
        let r = cmp_keys(before, after, &["x", "y"], &[]);
        assert!(r.duplicate_keys.is_empty(), "{:?}", r.duplicate_keys);
        assert_eq!(r.unchanged, 2);
    }

    #[test]
    fn an_ignored_column_cannot_make_a_row_changed() {
        let before = "id,v,updated
1,a,09:00
";
        let after = "id,v,updated
1,a,17:00
";
        assert_eq!(cmp_keys(before, after, &["id"], &[]).changed.len(), 1);

        let r = cmp_keys(before, after, &["id"], &["updated"]);
        assert!(r.changed.is_empty());
        assert_eq!(r.unchanged, 1);
    }

    #[test]
    fn ignoring_a_column_does_not_hide_a_real_change_elsewhere() {
        let before = "id,v,updated
1,a,09:00
";
        let after = "id,v,updated
1,CHANGED,17:00
";
        let r = cmp_keys(before, after, &["id"], &["updated"]);
        assert_eq!(r.changed.len(), 1);
        // Only the column that actually moved, not the ignored one.
        assert_eq!(r.changed[0].fields.len(), 1);
        assert_eq!(r.changed[0].fields[0].0, "v");
    }

    #[test]
    fn ignoring_a_column_that_does_not_exist_is_harmless() {
        let f = "id,v
1,a
";
        assert_eq!(cmp_keys(f, f, &["id"], &["nosuch"]).unchanged, 1);
    }

    #[test]
    fn a_missing_key_column_says_which_file() {
        let e = compare(
            &parse(
                "id,v
1,a
",
            )
            .unwrap(),
            &parse(
                "v
a
",
            )
            .unwrap(),
            &["id".into()],
            &[],
            Tolerance::Exact,
        )
        .unwrap_err();
        assert!(e.contains("second"), "{e}");
    }

    #[test]
    fn no_key_columns_is_an_error() {
        let t = parse(
            "id
1
",
        )
        .unwrap();
        assert!(compare(&t, &t, &[], &[], Tolerance::Exact).is_err());
    }
}
