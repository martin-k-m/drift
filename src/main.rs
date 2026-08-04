//! drift — diff two tabular datasets.

mod csv;
mod diff;

use std::process::ExitCode;

const USAGE: &str = "\
drift — diff two tabular datasets

usage:
  drift <before.csv> <after.csv> --key <column> [options]

options:
  -k, --key <columns>  column(s) that identify a row, comma-separated (required)
  -i, --ignore <cols>  columns to leave out of the comparison, comma-separated
  -e, --epsilon <n>    treat numbers within n of each other as unchanged
      --quiet          print nothing; use the exit code
      --no-color       plain output
  -h, --help           this
  -V, --version        version

exit codes:
  0  identical      1  differences found      2  could not run

Rows are matched on the key, not by position, so a row that only moved is
unchanged. Comparing exit code 1 is how a pipeline asks \"did this data move\".

--epsilon compares numerically where both sides are numbers, so a pipeline that
rewrites 0.1000001 as 0.1000002 stops reporting every row as changed. Text is
still compared exactly: a value that does not parse is never waved through.
";

struct Args {
    before: String,
    after: String,
    keys: Vec<String>,
    ignore: Vec<String>,
    quiet: bool,
    color: bool,
    tol: diff::Tolerance,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut positional = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut ignore: Vec<String> = Vec::new();
    let mut epsilon: Option<f64> = None;
    let mut quiet = false;
    // Colour off when the output is redirected: escape codes in a file someone
    // then diffs are noise. There is no isatty in std, so honour NO_COLOR and
    // let --no-color cover the rest.
    let mut color = std::env::var_os("NO_COLOR").is_none();

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("drift {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--quiet" => quiet = true,
            "--no-color" => color = false,
            "-k" | "--key" => {
                let raw = it.next().ok_or("--key needs a column name")?;
                keys.extend(split_columns(&raw));
            }
            "-i" | "--ignore" => {
                let raw = it.next().ok_or("--ignore needs a column name")?;
                ignore.extend(split_columns(&raw));
            }
            "-e" | "--epsilon" => {
                let raw = it.next().ok_or("--epsilon needs a number")?;
                let v: f64 = raw
                    .parse()
                    .map_err(|_| format!("--epsilon wants a number, got {raw:?}"))?;
                if !v.is_finite() || v < 0.0 {
                    return Err(format!("--epsilon must be zero or more, got {raw}"));
                }
                epsilon = Some(v);
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option {s}"));
            }
            s => positional.push(s.to_string()),
        }
    }

    if positional.len() != 2 {
        return Err(format!(
            "expected two files, got {}\n\n{USAGE}",
            positional.len()
        ));
    }
    if keys.is_empty() {
        return Err("--key is required: drift needs to know what identifies a row".into());
    }
    // A key column that is also ignored would be compared as identity and not
    // as a field, which is not wrong so much as a sign the command says
    // something its author did not mean.
    if let Some(k) = keys.iter().find(|k| ignore.contains(k)) {
        return Err(format!(
            "{k:?} is both a key and ignored; a key is never compared anyway"
        ));
    }

    Ok(Some(Args {
        before: positional[0].clone(),
        after: positional[1].clone(),
        keys,
        ignore,
        quiet,
        color,
        tol: match epsilon {
            Some(e) => diff::Tolerance::Absolute(e),
            None => diff::Tolerance::Exact,
        },
    }))
}

fn read(path: &str) -> Result<csv::Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    csv::parse(&text).map_err(|e| format!("{path}: {e}"))
}

/// Split a comma-separated column list, dropping blanks so `--key id,` and a
/// stray double comma are not silently turned into an empty column name.
fn split_columns(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(None) => return ExitCode::SUCCESS,
        Ok(Some(a)) => a,
        Err(e) => {
            eprintln!("drift: {e}");
            return ExitCode::from(2);
        }
    };

    let (before, after) = match (read(&args.before), read(&args.after)) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("drift: {e}");
            return ExitCode::from(2);
        }
    };

    let report = match diff::compare(&before, &after, &args.keys, &args.ignore, args.tol) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("drift: {e}");
            return ExitCode::from(2);
        }
    };

    if !args.quiet {
        print(&report, args.color);
    }

    if report.any() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

struct Paint(bool);

impl Paint {
    fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
    fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.0 {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

/// How many individual rows to name before summarising the rest.
const SHOWN: usize = 20;

fn print(r: &diff::Report, color: bool) {
    let c = Paint(color);

    if !r.any() {
        println!(
            "{}",
            c.dim(&format!("no differences · {} rows match", r.unchanged))
        );
        return;
    }

    if !r.added_columns.is_empty() || !r.removed_columns.is_empty() || r.reordered {
        println!("schema");
        for col in &r.removed_columns {
            println!("  {} column {col}", c.red("−"));
        }
        for col in &r.added_columns {
            println!("  {} column {col}", c.green("+"));
        }
        if r.reordered {
            println!("  {} columns reordered", c.yellow("~"));
        }
        println!();
    }

    if !r.changed.is_empty() {
        println!("changed · {}", r.changed.len());
        for ch in r.changed.iter().take(SHOWN) {
            println!("  {}", c.yellow(&diff::show_key(&ch.key)));
            for (col, b, a) in &ch.fields {
                println!("    {col}: {} → {}", c.red(b), c.green(a));
            }
        }
        if r.changed.len() > SHOWN {
            // Never silently truncate: a diff that hides rows without saying so
            // reads as a complete answer and is not one.
            println!(
                "  {}",
                c.dim(&format!("… and {} more", r.changed.len() - SHOWN))
            );
        }
        println!();
    }

    for (label, keys, mark) in [
        ("removed", &r.removed_rows, c.red("−")),
        ("added", &r.added_rows, c.green("+")),
    ] {
        if keys.is_empty() {
            continue;
        }
        println!("{label} · {}", keys.len());
        for k in keys.iter().take(SHOWN) {
            println!("  {mark} {}", diff::show_key(k));
        }
        if keys.len() > SHOWN {
            println!("  {}", c.dim(&format!("… and {} more", keys.len() - SHOWN)));
        }
        println!();
    }

    if !r.duplicate_keys.is_empty() {
        println!(
            "{}",
            c.yellow(&format!(
                "warning · {} duplicate key(s); comparison used one row each",
                r.duplicate_keys.len()
            ))
        );
        for k in r.duplicate_keys.iter().take(SHOWN) {
            println!("  {}", diff::show_key(k));
        }
        println!();
    }

    println!(
        "{}",
        c.dim(&format!(
            "{} unchanged · {} changed · {} added · {} removed",
            r.unchanged,
            r.changed.len(),
            r.added_rows.len(),
            r.removed_rows.len()
        ))
    );
}
