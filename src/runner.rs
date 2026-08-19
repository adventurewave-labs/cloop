//! Executes a loop: spawns `claude` repeatedly and evaluates the stop condition.

use crate::config::{self, LoopConfig, StopKind};
use crate::ui;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Print, without executing, the commands a run would issue.
pub fn preview_command(cfg: &LoopConfig) {
    ui::step("Dry run — commands that would execute:");
    println!();

    let p1 = config::render_prompt(cfg, 1, "");
    let a1 = config::claude_args(cfg, 1, &p1);
    println!("{}", ui::bold("iteration 1:"));
    println!("  {}", display_cmd(&a1));
    println!();

    let p2 = config::render_prompt(cfg, 2, "<last check output>");
    let a2 = config::claude_args(cfg, 2, &p2);
    println!("{}", ui::bold("iteration 2+:"));
    println!("  {}", display_cmd(&a2));

    if cfg.stop_kind == StopKind::Command {
        println!();
        println!("{}", ui::bold("stop check (each iteration):"));
        println!("  sh -c {}", shell_quote(&cfg.stop_command));
    }
    println!();
}

/// Run the loop. Returns a process exit code.
pub fn run(cfg: &LoopConfig, dry_run: bool) -> Result<i32, Box<dyn Error>> {
    let dir = Path::new(&cfg.workdir);
    if !dir.is_dir() {
        return Err(format!("working directory does not exist: {}", cfg.workdir).into());
    }

    ui::banner(&format!("cloop · {}", cfg.name));
    println!();
    print!("{}", config::summary(cfg));
    println!();

    if dry_run {
        preview_command(cfg);
        return Ok(0);
    }

    let start = Instant::now();
    let mut check_output = String::new();

    for iteration in 1..=cfg.max_iterations {
        let prompt = config::render_prompt(cfg, iteration, &check_output);
        let args = config::claude_args(cfg, iteration, &prompt);

        ui::step(&format!(
            "iteration {} / {}  {}",
            iteration,
            cfg.max_iterations,
            ui::dim(&format!("[{}]", elapsed(start)))
        ));
        println!("  {}", ui::dim(&display_cmd(&args)));
        println!();

        let mut child = match Command::new("claude")
            .args(&args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err("`claude` was not found on your PATH. Install Claude Code first: \
                            https://docs.claude.com/en/docs/claude-code"
                    .into());
            }
            Err(e) => return Err(e.into()),
        };

        let mut captured = String::new();
        if let Some(out) = child.stdout.take() {
            let reader = BufReader::new(out);
            for line in reader.lines() {
                let line = line?;
                println!("{}", line);
                captured.push_str(&line);
                captured.push('\n');
            }
        }

        let status = child.wait()?;
        if !status.success() {
            ui::warn(&format!(
                "claude exited with {} (continuing)",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "a signal".into())
            ));
        }
        println!();

        match cfg.stop_kind {
            StopKind::Marker => {
                if captured.contains(&cfg.stop_marker) {
                    ui::success(&format!(
                        "marker '{}' seen on iteration {} ({})",
                        cfg.stop_marker,
                        iteration,
                        elapsed(start)
                    ));
                    return Ok(0);
                }
            }
            StopKind::Command => {
                ui::info(&format!("running check: {}", cfg.stop_command));
                let out = Command::new("sh")
                    .arg("-c")
                    .arg(&cfg.stop_command)
                    .current_dir(dir)
                    .output()?;
                let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
                combined.push_str(&String::from_utf8_lossy(&out.stderr));
                check_output = combined;
                if out.status.success() {
                    ui::success(&format!(
                        "check passed on iteration {} ({})",
                        iteration,
                        elapsed(start)
                    ));
                    return Ok(0);
                } else {
                    ui::warn("check still failing");
                }
            }
            StopKind::Iterations => {}
        }

        if cfg.delay_secs > 0 && iteration < cfg.max_iterations {
            std::thread::sleep(Duration::from_secs(cfg.delay_secs));
        }
    }

    match cfg.stop_kind {
        StopKind::Iterations => {
            ui::success(&format!(
                "completed {} iterations ({})",
                cfg.max_iterations,
                elapsed(start)
            ));
            Ok(0)
        }
        _ => {
            ui::error(&format!(
                "reached the {}-iteration cap without satisfying the stop condition",
                cfg.max_iterations
            ));
            Ok(1)
        }
    }
}

fn elapsed(start: Instant) -> String {
    let secs = start.elapsed().as_secs();
    format!("{}m{:02}s", secs / 60, secs % 60)
}

/// Render args as a readable single-line command (truncating a long prompt).
fn display_cmd(args: &[String]) -> String {
    let mut parts: Vec<String> = vec!["claude".to_string()];
    for a in args {
        let one_line: String = a.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
        let shown = if one_line.chars().count() > 80 {
            let mut t: String = one_line.chars().take(77).collect();
            t.push('…');
            t
        } else {
            one_line
        };
        parts.push(shell_quote(&shown));
    }
    parts.join(" ")
}

/// Minimal shell quoting for display purposes only.
fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
