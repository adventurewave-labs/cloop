//! Tiny terminal UI helpers — colors and interactive prompts, zero dependencies.
//!
//! Everything here is built on std only. Colors use ANSI escape codes and are
//! suppressed when stdout is not a TTY or when `NO_COLOR` is set. Prompts are
//! numbered-menu style (no arrow keys); swapping in a crate like `inquire`
//! later would be a clean drop-in upgrade.

use std::io::{self, BufRead, IsTerminal, Write};

/// Whether ANSI color should be emitted on stdout.
pub fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    io::stdout().is_terminal()
}

/// Wrap `s` in an ANSI SGR code when color is enabled.
pub fn c(s: &str, code: &str) -> String {
    if color_enabled() {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    c(s, "1")
}
pub fn dim(s: &str) -> String {
    c(s, "2")
}
pub fn cyan(s: &str) -> String {
    c(s, "36")
}
pub fn green(s: &str) -> String {
    c(s, "32")
}
pub fn yellow(s: &str) -> String {
    c(s, "33")
}
pub fn red(s: &str) -> String {
    c(s, "31")
}
pub fn magenta(s: &str) -> String {
    c(s, "35")
}

/// Print a boxed banner. `title` should be plain text (no ANSI codes), since
/// the box width is measured by character count.
pub fn banner(title: &str) {
    let width = title.chars().count();
    let line = "─".repeat(width + 2);
    println!("{}", cyan(&format!("┌{}┐", line)));
    println!("{} {} {}", cyan("│"), bold(title), cyan("│"));
    println!("{}", cyan(&format!("└{}┘", line)));
}

pub fn info(msg: &str) {
    println!("{} {}", cyan("ℹ"), msg);
}
pub fn warn(msg: &str) {
    println!("{} {}", yellow("⚠"), msg);
}
pub fn success(msg: &str) {
    println!("{} {}", green("✓"), msg);
}
pub fn error(msg: &str) {
    eprintln!("{} {}", red("✗"), msg);
}
pub fn step(msg: &str) {
    println!("{} {}", magenta("▸"), bold(msg));
}

/// Read one line from stdin. Returns `Ok(None)` on EOF.
fn read_line_opt() -> io::Result<Option<String>> {
    let mut buf = String::new();
    let n = io::stdin().lock().read_line(&mut buf)?;
    if n == 0 {
        Ok(None)
    } else {
        while buf.ends_with('\n') || buf.ends_with('\r') {
            buf.pop();
        }
        Ok(Some(buf))
    }
}

fn flush() {
    let _ = io::stdout().flush();
}

/// Ask for free text.
///
/// * `default == None` — required; re-prompt until non-empty (errors on EOF).
/// * `default == Some("")` — optional; shows `(optional)`, may return "".
/// * `default == Some(x)` — shows `[x]`, returns `x` on empty input.
pub fn ask_text(prompt: &str, default: Option<&str>) -> io::Result<String> {
    loop {
        match default {
            None => print!("{} ", cyan(&format!("{}:", prompt))),
            Some("") => print!("{} {} ", cyan(&format!("{}:", prompt)), dim("(optional)")),
            Some(d) => print!(
                "{} {} ",
                cyan(&format!("{}:", prompt)),
                dim(&format!("[{}]", d))
            ),
        }
        flush();
        match read_line_opt()? {
            None => {
                println!();
                match default {
                    Some(d) => return Ok(d.to_string()),
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "input closed while a required value was expected",
                        ))
                    }
                }
            }
            Some(line) => {
                let line = line.trim();
                if line.is_empty() {
                    match default {
                        Some(d) => return Ok(d.to_string()),
                        None => {
                            warn("This field is required.");
                            continue;
                        }
                    }
                }
                return Ok(line.to_string());
            }
        }
    }
}

/// Ask a yes/no question.
pub fn ask_bool(prompt: &str, default: bool) -> io::Result<bool> {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    loop {
        print!("{} {} ", cyan(&format!("{}:", prompt)), dim(hint));
        flush();
        match read_line_opt()? {
            None => {
                println!();
                return Ok(default);
            }
            Some(line) => {
                let line = line.trim().to_lowercase();
                if line.is_empty() {
                    return Ok(default);
                }
                match line.as_str() {
                    "y" | "yes" => return Ok(true),
                    "n" | "no" => return Ok(false),
                    _ => warn("Please answer y or n."),
                }
            }
        }
    }
}

/// Ask for an unsigned integer.
pub fn ask_u64(prompt: &str, default: u64) -> io::Result<u64> {
    loop {
        print!(
            "{} {} ",
            cyan(&format!("{}:", prompt)),
            dim(&format!("[{}]", default))
        );
        flush();
        match read_line_opt()? {
            None => {
                println!();
                return Ok(default);
            }
            Some(line) => {
                let line = line.trim();
                if line.is_empty() {
                    return Ok(default);
                }
                match line.parse::<u64>() {
                    Ok(n) => return Ok(n),
                    Err(_) => warn("Please enter a whole number."),
                }
            }
        }
    }
}

/// Present a numbered menu and return the chosen index. `default` is 0-based.
pub fn ask_select(prompt: &str, options: &[&str], default: usize) -> io::Result<usize> {
    println!("{}", cyan(&format!("{}:", prompt)));
    for (i, opt) in options.iter().enumerate() {
        let marker = if i == default { green("●") } else { dim("○") };
        let num = if i == default {
            bold(&format!("{}", i + 1))
        } else {
            format!("{}", i + 1)
        };
        println!("  {} {} {}", marker, num, opt);
    }
    loop {
        print!("{} {} ", cyan("choose"), dim(&format!("[{}]", default + 1)));
        flush();
        match read_line_opt()? {
            None => {
                println!();
                return Ok(default);
            }
            Some(line) => {
                let line = line.trim();
                if line.is_empty() {
                    return Ok(default);
                }
                match line.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= options.len() => return Ok(n - 1),
                    _ => warn(&format!("Enter a number from 1 to {}.", options.len())),
                }
            }
        }
    }
}

/// Read a multi-line block. Ends on a line containing only `.` or on EOF.
pub fn ask_multiline(prompt: &str) -> io::Result<String> {
    println!(
        "{} {}",
        cyan(&format!("{}:", prompt)),
        dim("(end with a single '.' on its own line, or Ctrl-D)")
    );
    let mut lines: Vec<String> = Vec::new();
    loop {
        print!("{} ", dim("┃"));
        flush();
        match read_line_opt()? {
            None => break,
            Some(line) => {
                if line.trim() == "." {
                    break;
                }
                lines.push(line);
            }
        }
    }
    Ok(lines.join("\n"))
}
