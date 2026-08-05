//! Writing the report as JSON, by hand.
//!
//! `drift` has no dependencies, and that is the point of it: a tool that reads
//! two files and compares them should not pull in a tree you then have to
//! audit. Serialising one fixed shape is a page of code, against a derive macro
//! and everything behind it.
//!
//! The exit code was the only machine-readable thing here: `0` identical, `1`
//! differences, `2` could not run. That answers "did the data move" and nothing
//! else, so a pipeline wanting to know *what* moved had to parse output written
//! for a person, which is output that is allowed to change.
//!
//! # Escaping is the whole job
//!
//! The rest is punctuation. Cell values are arbitrary bytes from somebody's
//! export: quotes, backslashes, tabs, newlines, and control characters that are
//! legal in a CSV field and illegal raw in a JSON string. Getting this wrong
//! produces output that parses *most* of the time, which is worse than output
//! that never does, so [`escape`] is tested against each class directly.

use crate::diff::{Change, Report};

/// Render a report as a JSON object, newline-terminated.
///
/// The shape mirrors [`Report`] with one addition: `identical`, so a consumer
/// can branch without re-deriving it from four empty arrays and getting the
/// duplicate-key case wrong.
pub fn report(r: &Report, keys: &[String]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("{\n");

    push_field(&mut out, "identical", &bool_of(!r.any()), false);
    push_raw(&mut out, "keys", &array(keys));
    push_raw(&mut out, "addedColumns", &array(&r.added_columns));
    push_raw(&mut out, "removedColumns", &array(&r.removed_columns));
    push_field(&mut out, "columnsReordered", &bool_of(r.reordered), false);
    push_raw(&mut out, "addedRows", &key_array(&r.added_rows));
    push_raw(&mut out, "removedRows", &key_array(&r.removed_rows));
    push_raw(&mut out, "duplicateKeys", &key_array(&r.duplicate_keys));
    push_raw(&mut out, "changed", &changes(&r.changed));
    // Counts last, and `unchanged` without a trailing comma.
    out.push_str(&format!("  \"unchanged\": {}\n", r.unchanged));

    out.push_str("}\n");
    out
}

fn bool_of(b: bool) -> String {
    if b {
        "true".into()
    } else {
        "false".into()
    }
}

fn push_field(out: &mut String, name: &str, value: &str, quoted: bool) {
    if quoted {
        out.push_str(&format!("  \"{}\": \"{}\",\n", name, escape(value)));
    } else {
        out.push_str(&format!("  \"{name}\": {value},\n"));
    }
}

fn push_raw(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("  \"{name}\": {value},\n"));
}

/// A JSON array of strings.
fn array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let inner: Vec<String> = items.iter().map(|s| format!("\"{}\"", escape(s))).collect();
    format!("[{}]", inner.join(", "))
}

/// A key is several fields, so it serialises as an array of strings.
///
/// Not the joined display form: joining is for people, and a consumer that has
/// to split `"a · b"` back apart is one whose data cannot contain the
/// separator. The parts go over intact.
fn key_array(keys: &[Vec<String>]) -> String {
    if keys.is_empty() {
        return "[]".to_string();
    }
    let inner: Vec<String> = keys.iter().map(|k| array(k)).collect();
    format!("[{}]", inner.join(", "))
}

fn changes(changed: &[Change]) -> String {
    if changed.is_empty() {
        return "[]".to_string();
    }
    let mut out = String::from("[\n");
    for (i, c) in changed.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"key\": {},\n", array(&c.key)));
        out.push_str("      \"fields\": [\n");
        for (j, (column, before, after)) in c.fields.iter().enumerate() {
            out.push_str(&format!(
                "        {{ \"column\": \"{}\", \"before\": \"{}\", \"after\": \"{}\" }}",
                escape(column),
                escape(before),
                escape(after)
            ));
            if j + 1 < c.fields.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("      ]\n    }");
        if i + 1 < changed.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]");
    out
}

/// Escape a string for a JSON double-quoted context.
///
/// Every character below 0x20 has to go, not just the ones with short escapes:
/// a raw control byte in a string is invalid JSON, and CSV fields carry them.
/// Anything without a shorthand becomes a `\u00XX`.
///
/// The forward slash is deliberately not escaped. It is legal either way, and
/// escaping it makes every path in the output unreadable for no gain.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::parse;
    use crate::diff::{compare, Tolerance};

    fn report_for(before: &str, after: &str) -> String {
        let b = parse(before).unwrap();
        let a = parse(after).unwrap();
        let keys = vec!["id".to_string()];
        let r = compare(&b, &a, &keys, &[], Tolerance::Exact).unwrap();
        report(&r, &keys)
    }

    #[test]
    fn escapes_the_characters_that_would_break_the_output() {
        assert_eq!(escape("a\"b"), "a\\\"b");
        assert_eq!(escape("a\\b"), "a\\\\b");
        assert_eq!(escape("a\nb"), "a\\nb");
        assert_eq!(escape("a\rb"), "a\\rb");
        assert_eq!(escape("a\tb"), "a\\tb");
    }

    #[test]
    fn escapes_control_characters_without_a_shorthand() {
        // A raw control byte is invalid JSON and CSV fields carry them.
        assert_eq!(escape("a\u{0}b"), "a\\u0000b");
        assert_eq!(escape("a\u{1f}b"), "a\\u001fb");
        assert_eq!(escape("a\u{08}b"), "a\\bb");
        assert_eq!(escape("a\u{0c}b"), "a\\fb");
    }

    #[test]
    fn leaves_slashes_and_unicode_alone() {
        // Legal either way, and escaping them makes every path unreadable.
        assert_eq!(escape("a/b"), "a/b");
        assert_eq!(escape("café"), "café");
    }

    #[test]
    fn an_identical_pair_says_so() {
        let out = report_for("id,v\n1,a\n", "id,v\n1,a\n");
        assert!(out.contains("\"identical\": true"), "{out}");
        assert!(out.contains("\"unchanged\": 1"), "{out}");
    }

    #[test]
    fn a_changed_field_is_reported_with_both_values() {
        let out = report_for("id,v\n1,a\n", "id,v\n1,b\n");
        assert!(out.contains("\"identical\": false"), "{out}");
        assert!(out.contains("\"column\": \"v\""), "{out}");
        assert!(out.contains("\"before\": \"a\""), "{out}");
        assert!(out.contains("\"after\": \"b\""), "{out}");
    }

    #[test]
    fn added_and_removed_rows_carry_their_key_parts() {
        let out = report_for("id,v\n1,a\n", "id,v\n2,a\n");
        assert!(out.contains("\"addedRows\": [[\"2\"]]"), "{out}");
        assert!(out.contains("\"removedRows\": [[\"1\"]]"), "{out}");
    }

    #[test]
    fn a_key_is_an_array_rather_than_a_joined_string() {
        // A consumer that has to split "a · b" back apart is one whose data
        // cannot contain the separator.
        let b = parse("a,b,v\nx,y,1\n").unwrap();
        let a = parse("a,b,v\nx,y,2\n").unwrap();
        let keys = vec!["a".to_string(), "b".to_string()];
        let r = compare(&b, &a, &keys, &[], Tolerance::Exact).unwrap();
        let out = report(&r, &keys);
        assert!(out.contains("\"key\": [\"x\", \"y\"]"), "{out}");
    }

    #[test]
    fn schema_changes_are_reported() {
        let out = report_for("id,v\n1,a\n", "id,v,extra\n1,a,z\n");
        assert!(out.contains("\"addedColumns\": [\"extra\"]"), "{out}");
        assert!(out.contains("\"removedColumns\": []"), "{out}");
    }

    #[test]
    fn a_value_containing_a_quote_survives_the_round_trip() {
        // The case that produces output which parses most of the time.
        let out = report_for("id,v\n1,plain\n", "id,v\n1,\"say \"\"hi\"\"\"\n");
        assert!(out.contains("say \\\"hi\\\""), "{out}");
    }

    #[test]
    fn the_output_is_one_object_ending_in_a_newline() {
        let out = report_for("id,v\n1,a\n", "id,v\n1,a\n");
        assert!(out.starts_with('{'));
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn no_field_is_followed_by_a_trailing_comma() {
        // The classic hand-rolled-JSON bug: valid to a lenient reader and
        // rejected by a strict one.
        let out = report_for("id,v\n1,a\n", "id,v\n1,b\n");
        assert!(!out.contains(",\n}"), "{out}");
        assert!(!out.contains(",]"), "{out}");
        assert!(!out.contains(", ]"), "{out}");
        assert!(!out.contains(",}"), "{out}");
    }
}
