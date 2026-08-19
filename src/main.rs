//! cloop — wrap Claude Code's headless mode in a configurable agentic loop.
//!
//! Claude Code's `--print` mode is a single batch turn: one prompt in, one
//! result out, an exit code you can branch on. That primitive is meant to be
//! composed into loops — run it, check whether you're done, run it again with
//! feedback. cloop is that outer loop, made reusable and saveable.

mod config;
mod export;
mod runner;
mod ui;
mod wizard;

use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(code) => code,
        Err(e) => {
            ui::error(&e.to_string());
            1
        }
    };
    std::process::exit(code);
}

fn run(args: &[String]) -> Result<i32, Box<dyn Error>> {
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "" => cmd_new(true),
        "new" => cmd_new(false),
        "run" => cmd_run(&args[1..]),
        "list" | "ls" => cmd_list(),
        "show" | "cat" => cmd_show(args.get(1).map(|s| s.as_str())),
        "export" => cmd_export(args.get(1).map(|s| s.as_str())),
        "edit" => cmd_edit(args.get(1).map(|s| s.as_str())),
        "rm" | "remove" => cmd_rm(args.get(1).map(|s| s.as_str())),
        "-h" | "--help" | "help" => {
            print_help();
            Ok(0)
        }
        "-V" | "--version" => {
            println!("cloop {}", VERSION);
            Ok(0)
        }
        other => {
            ui::error(&format!("unknown command: {}", other));
            println!();
            print_help();
            Ok(2)
        }
    }
}

// ---- storage --------------------------------------------------------------

fn config_dir() -> PathBuf {
    if let Some(d) = env::var_os("CLOOP_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = env::var_os("XDG_CONFIG_HOME") {
        let mut p = PathBuf::from(d);
        p.push("cloop");
        return p;
    }
    if let Some(home) = env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".config");
        p.push("cloop");
        return p;
    }
    PathBuf::from(".cloop")
}

fn loops_dir() -> PathBuf {
    let mut p = config_dir();
    p.push("loops");
    p
}

fn loop_path(name: &str) -> PathBuf {
    let mut p = loops_dir();
    p.push(format!("{}.toml", name));
    p
}

fn list_names() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(loops_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

fn load(name: &str) -> Result<config::LoopConfig, Box<dyn Error>> {
    let path = loop_path(name);
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("no loop named '{}' ({})", name, path.display()))?;
    config::from_toml(&text).map_err(|e| e.into())
}

fn save(cfg: &config::LoopConfig, confirm_overwrite: bool) -> Result<PathBuf, Box<dyn Error>> {
    let dir = loops_dir();
    fs::create_dir_all(&dir)?;
    let path = loop_path(&cfg.name);
    if confirm_overwrite && path.exists() {
        let ok = ui::ask_bool(
            &format!("Loop '{}' already exists. Overwrite?", cfg.name),
            false,
        )?;
        if !ok {
            return Err("aborted — not overwritten".into());
        }
    }
    fs::write(&path, config::to_toml(cfg))?;
    Ok(path)
}

// ---- commands -------------------------------------------------------------

fn cmd_new(offer_run: bool) -> Result<i32, Box<dyn Error>> {
    let cfg = wizard::run_wizard("my-loop")?;

    println!();
    ui::step("Summary");
    print!("{}", config::summary(&cfg));
    println!();

    let path = save(&cfg, true)?;
    ui::success(&format!("saved to {}", path.display()));
    println!();

    if offer_run {
        let dry = ui::ask_bool("Preview the commands first (dry run)?", true)?;
        if dry {
            println!();
            runner::run(&cfg, true)?;
            println!();
        }
        let go = ui::ask_bool("Run the loop now?", !cfg.skip_permissions)?;
        if go {
            println!();
            return runner::run(&cfg, false);
        }
    }

    ui::info(&format!("Run it later with:  cloop run {}", cfg.name));
    Ok(0)
}

fn cmd_run(args: &[String]) -> Result<i32, Box<dyn Error>> {
    let mut name: Option<String> = None;
    let mut config_path: Option<String> = None;
    let mut dry_run = false;
    let mut yes = false;
    let mut max_override: Option<u64> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                config_path = Some(args.get(i).ok_or("--config needs a path")?.clone());
            }
            "-n" | "--dry-run" => dry_run = true,
            "-y" | "--yes" => yes = true,
            "--max" => {
                i += 1;
                max_override = Some(
                    args.get(i)
                        .ok_or("--max needs a number")?
                        .parse::<u64>()
                        .map_err(|_| "--max needs a number")?,
                );
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown flag for run: {}", other).into());
            }
            other => name = Some(other.to_string()),
        }
        i += 1;
    }

    let mut cfg = if let Some(p) = config_path {
        let text = fs::read_to_string(&p).map_err(|_| format!("cannot read config: {}", p))?;
        config::from_toml(&text)?
    } else if let Some(n) = name {
        load(&n)?
    } else {
        let names = list_names();
        if names.is_empty() {
            return Err("no saved loops yet — run `cloop new` to create one".into());
        }
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let idx = ui::ask_select("Which loop?", &refs, 0)?;
        load(&names[idx])?
    };

    if let Some(m) = max_override {
        cfg.max_iterations = m;
    }

    if !yes && !dry_run {
        ui::banner(&format!("cloop · {}", cfg.name));
        println!();
        print!("{}", config::summary(&cfg));
        println!();
        if cfg.skip_permissions {
            ui::warn(&ui::red(
                "This loop bypasses ALL permission checks. Be sure you're in a sandbox.",
            ));
        }
        let go = ui::ask_bool("Start the loop?", !cfg.skip_permissions)?;
        if !go {
            ui::info("cancelled");
            return Ok(0);
        }
        println!();
    }

    runner::run(&cfg, dry_run)
}

fn cmd_list() -> Result<i32, Box<dyn Error>> {
    let names = list_names();
    if names.is_empty() {
        ui::info("no saved loops yet — run `cloop new` to create one");
        return Ok(0);
    }
    ui::step("Saved loops");
    for name in names {
        let task_line = load(&name)
            .ok()
            .and_then(|c| c.task.lines().next().map(|s| s.to_string()))
            .unwrap_or_default();
        println!("  {}  {}", ui::cyan(&name), ui::dim(&task_line));
    }
    Ok(0)
}

fn cmd_show(name: Option<&str>) -> Result<i32, Box<dyn Error>> {
    let name = name.ok_or("usage: cloop show <name>")?;
    let cfg = load(name)?;
    ui::banner(&format!("cloop · {}", cfg.name));
    println!();
    print!("{}", config::summary(&cfg));
    println!();
    ui::step("Task");
    println!("{}", cfg.task);
    println!();
    ui::info(&format!("file: {}", loop_path(name).display()));
    Ok(0)
}

fn cmd_export(name: Option<&str>) -> Result<i32, Box<dyn Error>> {
    let name = name.ok_or("usage: cloop export <name>")?;
    let cfg = load(name)?;
    print!("{}", export::to_bash(&cfg));
    Ok(0)
}

fn cmd_edit(name: Option<&str>) -> Result<i32, Box<dyn Error>> {
    let name = name.ok_or("usage: cloop edit <name>")?;
    let path = loop_path(name);
    if !path.exists() {
        return Err(format!("no loop named '{}'", name).into());
    }
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = Command::new(&editor).arg(&path).status()?;
    if !status.success() {
        return Err(format!("{} exited with an error", editor).into());
    }
    match load(name) {
        Ok(_) => ui::success("saved and valid"),
        Err(e) => ui::warn(&format!("saved, but parsing reported: {}", e)),
    }
    Ok(0)
}

fn cmd_rm(name: Option<&str>) -> Result<i32, Box<dyn Error>> {
    let name = name.ok_or("usage: cloop rm <name>")?;
    let path = loop_path(name);
    if !path.exists() {
        return Err(format!("no loop named '{}'", name).into());
    }
    let ok = ui::ask_bool(&format!("Delete loop '{}'?", name), false)?;
    if !ok {
        ui::info("kept");
        return Ok(0);
    }
    fs::remove_file(&path)?;
    ui::success(&format!("deleted '{}'", name));
    Ok(0)
}

fn print_help() {
    println!("{}", ui::bold(&format!("cloop {}", VERSION)));
    println!("Wrap Claude Code's headless mode in a configurable agentic loop.");
    println!();
    println!("{}", ui::bold("USAGE"));
    println!("  cloop                      run the wizard, then optionally start");
    println!("  cloop new                  create and save a loop");
    println!("  cloop run [name]           run a saved loop (prompts if no name)");
    println!("  cloop list                 list saved loops");
    println!("  cloop show <name>          print a loop's settings");
    println!("  cloop export <name>        print an equivalent bash script");
    println!("  cloop edit <name>          open a loop in $EDITOR");
    println!("  cloop rm <name>            delete a loop");
    println!();
    println!("{}", ui::bold("RUN FLAGS"));
    println!("  -n, --dry-run              show the commands without executing");
    println!("  -y, --yes                  skip the confirmation prompt");
    println!("      --max <n>              override the iteration cap");
    println!("      --config <path>        run a loop file directly (unsaved)");
    println!();
    println!("{}", ui::bold("STORAGE"));
    println!("  Loops live in $CLOOP_DIR or ~/.config/cloop/loops/*.toml");
    println!();
    println!("{}", ui::bold("EXAMPLES"));
    println!("  cloop                      # interactive");
    println!("  cloop run fix-tests -y     # rerun a saved loop, no prompt");
    println!("  cloop run fix-tests -n     # preview only");
}
