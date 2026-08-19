//! Interactive wizard that builds a `LoopConfig` by asking questions.

use crate::config::{default_iter_prompt, LoopConfig, StopKind};
use crate::ui;
use std::io;

/// Turn arbitrary text into a safe lowercase filename stem.
fn sanitize_name(s: &str) -> String {
    let mut out = String::new();
    for ch in s.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
        // anything else is dropped
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "loop".to_string()
    } else {
        trimmed
    }
}

fn indent(s: &str, pad: &str) -> String {
    s.lines()
        .map(|l| format!("{}{}", pad, l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run the wizard. Returns a fully-populated config.
pub fn run_wizard(default_name: &str) -> io::Result<LoopConfig> {
    ui::banner("cloop · new loop");
    println!();
    ui::info("A loop runs Claude Code headless, over and over, until it's done.");
    println!();

    let mut cfg = LoopConfig::default();

    let raw_name = ui::ask_text("Loop name", Some(default_name))?;
    cfg.name = sanitize_name(&raw_name);

    println!();
    cfg.task = ui::ask_multiline("Task — what should Claude do?")?;

    println!();
    cfg.workdir = ui::ask_text("Working directory", Some("."))?;

    println!();
    let models = [
        "default (Claude Code's configured model)",
        "sonnet",
        "opus",
        "haiku",
        "custom…",
    ];
    let m = ui::ask_select("Model", &models, 0)?;
    cfg.model = match m {
        0 => String::new(),
        1 => "sonnet".into(),
        2 => "opus".into(),
        3 => "haiku".into(),
        _ => ui::ask_text("Model name", None)?,
    };

    println!();
    let stops = [
        "command — loop until a shell command exits 0 (e.g. tests pass)",
        "marker — loop until Claude prints a marker string",
        "iterations — run a fixed number of times",
    ];
    let s = ui::ask_select("Stop condition", &stops, 0)?;
    cfg.stop_kind = match s {
        1 => StopKind::Marker,
        2 => StopKind::Iterations,
        _ => StopKind::Command,
    };
    match cfg.stop_kind {
        StopKind::Command => {
            cfg.stop_command = ui::ask_text("Check command", Some("cargo test"))?;
        }
        StopKind::Marker => {
            cfg.stop_marker = ui::ask_text("Marker string", Some("LOOP_DONE"))?;
        }
        StopKind::Iterations => {}
    }

    println!();
    let cap_label = if cfg.stop_kind == StopKind::Iterations {
        "How many iterations?"
    } else {
        "Max iterations (safety cap)"
    };
    cfg.max_iterations = ui::ask_u64(cap_label, 25)?;

    if cfg.stop_kind != StopKind::Iterations {
        cfg.carry_context = ui::ask_bool("Carry conversation context across iterations?", true)?;
    }

    println!();
    let perms = [
        "default — Claude asks before risky actions",
        "acceptEdits — auto-accept file edits",
        "plan — planning only, no changes",
        "dontAsk — unattended-safe, fewer prompts",
        "bypassPermissions — skip ALL checks (containers/VMs only)",
    ];
    let p = ui::ask_select("Permission mode", &perms, 0)?;
    match p {
        0 => {}
        1 => cfg.permission_mode = "acceptEdits".into(),
        2 => cfg.permission_mode = "plan".into(),
        3 => cfg.permission_mode = "dontAsk".into(),
        _ => {
            ui::warn(&ui::red(
                "bypassPermissions removes all safety checks. Use only in a sandbox.",
            ));
            cfg.skip_permissions = true;
        }
    }

    println!();
    let advanced = ui::ask_bool("Configure advanced options?", false)?;
    if advanced {
        cfg.max_turns = ui::ask_u64("Max agentic turns per run (0 = unlimited)", 0)?;
        cfg.allowed_tools = ui::ask_text("Allowed tools (comma-separated, blank = all)", Some(""))?;
        cfg.delay_secs = ui::ask_u64("Delay between iterations (seconds)", 0)?;
        cfg.extra_args = ui::ask_text("Extra claude flags (advanced)", Some(""))?;
    }

    let default_prompt = default_iter_prompt(cfg.stop_kind);
    println!();
    ui::info("Per-iteration prompt (used from iteration 2 on). Default:");
    println!("{}", ui::dim(&indent(&default_prompt, "    ")));
    let customize = ui::ask_bool("Customize this prompt?", false)?;
    cfg.iter_prompt = if customize {
        ui::ask_multiline("Enter your per-iteration prompt")?
    } else {
        default_prompt
    };

    Ok(cfg)
}
