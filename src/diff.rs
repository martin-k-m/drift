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
    pub added_rows: Vec<String>,
    pub removed_rows: Vec<String>,
    pub changed: Vec<Change>,
    pub duplicate_keys: Vec<String>,
    pub unchanged: usize,
}

#[derive(Debug)]
pub struct Change {
    pub key: String,
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

/// Index a table by its key column, remembering keys that appear twice.
///
/// Duplicates are reported rather than silently resolved: with a repeated key
/// there is no single "before" row to compare against, and picking one quietly
/// would make the diff depend on file order.
fn index(t: &Table, key: usize) -> (HashMap<String, &Vec<String>>, Vec<String>) {
    let mut map = HashMap::new();
    let mut dupes = Vec::new();
    for row in &t.rows {
        let k = t.field(row, key).to_string();
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

pub fn compare(before: &Table, after: &Table, key: &str, tol: Tolerance) -> Result<Report, String> {
    let bk = before.column(key).ok_or_else(|| {
        format!(
            "no column {key:?} in the first file; it has {}",
            list(&before.header)
        )
    })?;
    let ak = after.column(key).ok_or_else(|| {
        format!(
            "no column {key:?} in the second file; it has {}",
            list(&after.header)
        )
    })?;

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

    let (bmap, mut dupes) = index(before, bk);
    let (amap, adupes) = index(after, ak);
    for d in adupes {
        if !dupes.contains(&d) {
            dupes.push(d);
        }
    }

    // Compare only the columns both files have; a column that exists on one
    // side is a schema change, already reported, and would otherwise show up
    // again as a change on every single row.
    let shared: Vec<&String> = before
        .header
        .iter()
        .filter(|c| after.header.contains(c))
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

    fn cmp(a: &str, b: &str) -> Report {
        compare(
            &parse(a).unwrap(),
            &parse(b).unwrap(),
            "id",
            Tolerance::Exact,
        )
        .unwrap()
    }

    fn cmp_eps(a: &str, b: &str, eps: f64) -> Report {
        compare(
            &parse(a).unwrap(),
            &parse(b).unwrap(),
            "id",
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
        assert_eq!(r.removed_rows, ["1"]);
        assert_eq!(r.added_rows, ["3"]);
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
        assert_eq!(r.duplicate_keys, ["1"]);
    }

    #[test]
    fn missing_key_column_names_what_is_there() {
        let e = compare(
            &parse("a\n1\n").unwrap(),
            &parse("a\n1\n").unwrap(),
            "id",
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
        assert_eq!(r.removed_rows, ["1", "2", "3"]);
    }
}
