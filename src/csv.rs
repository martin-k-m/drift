//! A CSV reader, because the alternative is a dependency tree for a hundred
//! lines of work.
//!
//! Covers RFC 4180 as it actually appears: quoted fields, doubled quotes inside
//! them, embedded commas and newlines, and both line endings. It does not do
//! custom delimiters, comments, or encoding detection, those are the features
//! that turn a parser into a library, and this is not trying to be one.

/// One parsed file: a header and the rows under it.
#[derive(Debug)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Index of a named column.
    pub fn column(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|h| h == name)
    }

    /// A field, or "" when the row is short.
    ///
    /// Ragged rows are common in exported data and are not worth refusing to
    /// run over; a missing field reads as empty, which is what a diff should
    /// then report as a change.
    pub fn field<'a>(&self, row: &'a [String], idx: usize) -> &'a str {
        row.get(idx).map(String::as_str).unwrap_or("")
    }
}

/// Parse a whole CSV document.
pub fn parse(input: &str) -> Result<Table, String> {
    // A leading UTF-8 BOM is common in exports (Excel writes one) and would
    // otherwise glue itself to the first column name, so `--key id` fails with
    // "no column id" over a file that plainly has one. Drop a single leading
    // BOM. This is not encoding detection: the bytes are still read as UTF-8.
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);

    let mut records = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    let mut line = 1usize;

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    // A doubled quote inside a quoted field is one literal
                    // quote; a single one closes the field.
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                _ => field.push(c),
            }
            continue;
        }

        match c {
            '"' if field.is_empty() => in_quotes = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {
                // Swallow the \n of a \r\n pair so it is one line break.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            '\n' => {
                line += 1;
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(c),
        }
    }

    if in_quotes {
        return Err(format!("unterminated quote starting before line {line}"));
    }

    // A file not ending in a newline still has a final record.
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    // Trailing blank lines are noise, not empty rows.
    while matches!(records.last(), Some(r) if r.len() == 1 && r[0].is_empty()) {
        records.pop();
    }

    let mut it = records.into_iter();
    let header = it.next().ok_or_else(|| "file is empty".to_string())?;
    Ok(Table {
        header,
        rows: it.collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_rows() {
        let t = parse("a,b\n1,2\n3,4\n").unwrap();
        assert_eq!(t.header, ["a", "b"]);
        assert_eq!(t.rows, [["1", "2"], ["3", "4"]]);
    }

    #[test]
    fn quoted_commas_and_newlines_stay_in_the_field() {
        let t = parse("a,b\n\"x,y\",\"line\nbreak\"\n").unwrap();
        assert_eq!(t.rows[0], ["x,y", "line\nbreak"]);
    }

    #[test]
    fn doubled_quote_is_one_quote() {
        let t = parse("a\n\"he said \"\"hi\"\"\"\n").unwrap();
        assert_eq!(t.rows[0][0], r#"he said "hi""#);
    }

    #[test]
    fn crlf_is_one_line_break() {
        let t = parse("a,b\r\n1,2\r\n").unwrap();
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0], ["1", "2"]);
    }

    #[test]
    fn no_trailing_newline_still_yields_the_last_row() {
        let t = parse("a,b\n1,2").unwrap();
        assert_eq!(t.rows, [["1", "2"]]);
    }

    #[test]
    fn ragged_rows_read_short_fields_as_empty() {
        let t = parse("a,b,c\n1,2\n").unwrap();
        assert_eq!(t.field(&t.rows[0], 2), "");
    }

    #[test]
    fn unterminated_quote_is_an_error_not_a_panic() {
        assert!(parse("a\n\"oops\n").unwrap_err().contains("unterminated"));
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(parse("").is_err());
    }

    #[test]
    fn a_leading_bom_is_dropped_so_the_first_column_is_named() {
        // Excel-style export: a UTF-8 BOM ahead of the header. Without dropping
        // it the first column reads as "\u{feff}id" and no key matches.
        let t = parse("\u{feff}id,v\n1,a\n").unwrap();
        assert_eq!(t.header, ["id", "v"]);
        assert_eq!(t.column("id"), Some(0));
    }

    #[test]
    fn only_one_leading_bom_is_dropped() {
        // A second BOM is data, not a marker, and stays on the field.
        let t = parse("\u{feff}\u{feff}id\n1\n").unwrap();
        assert_eq!(t.header, ["\u{feff}id"]);
    }

    #[test]
    fn a_bare_cr_is_a_line_break() {
        // Old-Mac line endings: a lone \r with no following \n still ends a row.
        let t = parse("a,b\r1,2\r").unwrap();
        assert_eq!(t.rows, [["1", "2"]]);
    }

    #[test]
    fn a_quoted_field_may_be_empty() {
        let t = parse("a,b\n\"\",x\n").unwrap();
        assert_eq!(t.rows[0], ["", "x"]);
    }

    #[test]
    fn a_quoted_field_may_hold_a_comma_only() {
        let t = parse("a\n\",\"\n").unwrap();
        assert_eq!(t.rows[0], [","]);
    }

    #[test]
    fn trailing_blank_lines_are_not_rows() {
        let t = parse("a\n1\n\n\n").unwrap();
        assert_eq!(t.rows, [["1"]]);
    }

    #[test]
    fn a_header_only_file_has_no_rows() {
        let t = parse("a,b\n").unwrap();
        assert_eq!(t.header, ["a", "b"]);
        assert!(t.rows.is_empty());
    }
}
