//! Loop configuration: the data model, a tiny TOML-subset (de)serializer,
//! Claude Code argument construction, and per-iteration prompt rendering.

use crate::ui;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind { Iterations, Marker, Command }

impl StopKind {
    pub fn as_str(self) -> &'static str {
        match self { StopKind::Iterations => "iterations", StopKind::Marker => "marker", StopKind::Command => "command" }
    }
    pub fn from_str(s: &str) -> StopKind {
        match s { "iterations" => StopKind::Iterations, "marker" => StopKind::Marker, _ => StopKind::Command }
    }
}

#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub name: String, pub task: String, pub workdir: String, pub model: String,
    pub stop_kind: StopKind, pub stop_command: String, pub stop_marker: String,
    pub max_iterations: u64, pub carry_context: bool, pub permission_mode: String,
    pub skip_permissions: bool, pub max_turns: u64, pub allowed_tools: String,
    pub delay_secs: u64, pub extra_args: String, pub iter_prompt: String,
}

impl Default for LoopConfig {
    fn default() -> Self {
        LoopConfig { name: String::new(), task: String::new(), workdir: ".".to_string(), model: String::new(),
            stop_kind: StopKind::Command, stop_command: String::new(), stop_marker: "LOOP_DONE".to_string(),
            max_iterations: 25, carry_context: true, permission_mode: String::new(), skip_permissions: false,
            max_turns: 0, allowed_tools: String::new(), delay_secs: 0, extra_args: String::new(), iter_prompt: String::new() }
    }
}

pub fn default_iter_prompt(kind: StopKind) -> String {
    match kind {
        StopKind::Command => "Iteration {iteration} of {max}.\n\nThe check command `{stop_command}` is still failing. Its latest output was:\n\n{check_output}\n\nDiagnose why it is failing and fix it. Make concrete edits — do not just describe the problem. When you believe it is resolved, stop.".to_string(),
        StopKind::Marker => "Iteration {iteration} of {max}.\n\nContinue working on the task:\n\n{task}\n\nWhen the task is fully complete, print the exact line {stop_marker} on its own line and nothing after it.".to_string(),
        StopKind::Iterations => "Iteration {iteration} of {max}. Continue working on the task:\n\n{task}".to_string(),
    }
}

pub fn claude_args(cfg: &LoopConfig, iteration: u64, prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    args.push("--print".to_string());
    if iteration > 1 && cfg.carry_context { args.push("--continue".to_string()); }
    if !cfg.model.is_empty() { args.push("--model".to_string()); args.push(cfg.model.clone()); }
    if cfg.skip_permissions { args.push("--dangerously-skip-permissions".to_string()); }
    else if !cfg.permission_mode.is_empty() { args.push("--permission-mode".to_string()); args.push(cfg.permission_mode.clone()); }
    if cfg.max_turns > 0 { args.push("--max-turns".to_string()); args.push(cfg.max_turns.to_string()); }
    if !cfg.allowed_tools.is_empty() { args.push("--allowedTools".to_string()); args.push(cfg.allowed_tools.clone()); }
    for a in split_args(&cfg.extra_args) { args.push(a); }
    args.push(prompt.to_string());
    args
}

pub fn render_prompt(cfg: &LoopConfig, iteration: u64, check_output: &str) -> String {
    if iteration <= 1 {
        if cfg.stop_kind == StopKind::Marker {
            return format!("{}\n\nWhen the task is fully complete, print the exact line {} on its own line and nothing after it.", cfg.task, cfg.stop_marker);
        }
        return cfg.task.clone();
    }
    let template = if cfg.iter_prompt.is_empty() { default_iter_prompt(cfg.stop_kind) } else { cfg.iter_prompt.clone() };
    template.replace("{iteration}", &iteration.to_string()).replace("{max}", &cfg.max_iterations.to_string())
        .replace("{stop_command}", &cfg.stop_command).replace("{stop_marker}", &cfg.stop_marker)
        .replace("{check_output}", check_output).replace("{task}", &cfg.task)
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_single = false; let mut in_double = false; let mut has_token = false;
    for ch in s.chars() {
        if in_single { if ch == '\'' { in_single = false; } else { cur.push(ch); } }
        else if in_double { if ch == '"' { in_double = false; } else { cur.push(ch); } }
        else if ch == '\'' { in_single = true; has_token = true; }
        else if ch == '"' { in_double = true; has_token = true; }
        else if ch.is_whitespace() { if has_token { out.push(std::mem::take(&mut cur)); has_token = false; } }
        else { cur.push(ch); has_token = true; }
    }
    if has_token { out.push(cur); }
    out
}

fn esc_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch { '\\' => out.push_str("\\\\"), '"' => out.push_str("\\\""), '\n' => out.push_str("\\n"), '\t' => out.push_str("\\t"), '\r' => out.push_str("\\r"), _ => out.push(ch) }
    }
    out
}

fn unescape_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'), Some('"') => out.push('"'), Some('n') => out.push('\n'),
                Some('t') => out.push('\t'), Some('r') => out.push('\r'),
                Some(other) => { out.push('\\'); out.push(other); }, None => out.push('\\'),
            }
        } else { out.push(ch); }
    }
    out
}

fn emit_multiline(out: &mut String, key: &str, val: &str) {
    out.push_str(key); out.push_str(" = \"\"\"\n"); out.push_str(val); out.push('\n'); out.push_str("\"\"\"\n");
}

pub fn to_toml(cfg: &LoopConfig) -> String {
    let mut out = String::new();
    out.push_str("# cloop loop definition\n");
    out.push_str(&format!("name = \"{}\"\n\n", esc_basic(&cfg.name)));
    out.push_str("# The task Claude works on (used verbatim on iteration 1).\n");
    emit_multiline(&mut out, "task", &cfg.task);
    out.push('\n');
    out.push_str(&format!("workdir = \"{}\"\n", esc_basic(&cfg.workdir)));
    out.push_str("# model: \"\" uses Claude Code's configured default.\n");
    out.push_str(&format!("model = \"{}\"\n", esc_basic(&cfg.model)));
    out.push('\n');
    out.push_str("# Stop condition: \"command\", \"marker\", or \"iterations\".\n");
    out.push_str(&format!("stop_kind = \"{}\"\n", cfg.stop_kind.as_str()));
    out.push_str(&format!("stop_command = \"{}\"\n", esc_basic(&cfg.stop_command)));
    out.push_str(&format!("stop_marker = \"{}\"\n", esc_basic(&cfg.stop_marker)));
    out.push('\n');
    out.push_str(&format!("max_iterations = {}\n", cfg.max_iterations));
    out.push_str(&format!("carry_context = {}\n", cfg.carry_context));
    out.push('\n');
    out.push_str("# Permissions. skip_permissions maps to --dangerously-skip-permissions.\n");
    out.push_str(&format!("permission_mode = \"{}\"\n", esc_basic(&cfg.permission_mode)));
    out.push_str(&format!("skip_permissions = {}\n", cfg.skip_permissions));
    out.push('\n');
    out.push_str("# Extra controls (0 / \"\" = unset).\n");
    out.push_str(&format!("max_turns = {}\n", cfg.max_turns));
    out.push_str(&format!("allowed_tools = \"{}\"\n", esc_basic(&cfg.allowed_tools)));
    out.push_str(&format!("delay_secs = {}\n", cfg.delay_secs));
    out.push_str(&format!("extra_args = \"{}\"\n", esc_basic(&cfg.extra_args)));
    out.push('\n');
    out.push_str("# Per-iteration prompt template (used from iteration 2 on).\n");
    out.push_str("# Placeholders: {iteration} {max} {task} {stop_command} {stop_marker} {check_output}\n");
    emit_multiline(&mut out, "iter_prompt", &cfg.iter_prompt);
    out
}

pub fn from_toml(text: &str) -> Result<LoopConfig, String> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut lines = text.lines();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let eq = match line.find('=') { Some(i) => i, None => continue };
        let key = line[..eq].trim().to_string();
        let val_part = line[eq + 1..].trim();
        if val_part == "\"\"\"" {
            let mut body: Vec<String> = Vec::new();
            loop {
                match lines.next() {
                    Some(l) => { if l.trim() == "\"\"\"" { break; } body.push(l.to_string()); }
                    None => return Err(format!("unterminated multi-line value for `{}`", key)),
                }
            }
            map.insert(key, body.join("\n"));
        } else if val_part.len() >= 2 && val_part.starts_with('"') && val_part.ends_with('"') {
            let inner = &val_part[1..val_part.len() - 1];
            map.insert(key, unescape_basic(inner));
        } else { map.insert(key, val_part.to_string()); }
    }
    let mut cfg = LoopConfig::default();
    if let Some(v) = map.get("name") { cfg.name = v.clone(); }
    if let Some(v) = map.get("task") { cfg.task = v.clone(); }
    if let Some(v) = map.get("workdir") { if !v.is_empty() { cfg.workdir = v.clone(); } }
    if let Some(v) = map.get("model") { cfg.model = v.clone(); }
    if let Some(v) = map.get("stop_kind") { cfg.stop_kind = StopKind::from_str(v); }
    if let Some(v) = map.get("stop_command") { cfg.stop_command = v.clone(); }
    if let Some(v) = map.get("stop_marker") { if !v.is_empty() { cfg.stop_marker = v.clone(); } }
    if let Some(v) = map.get("max_iterations") { if let Ok(n) = v.trim().parse::<u64>() { cfg.max_iterations = n; } }
    if let Some(v) = map.get("carry_context") { cfg.carry_context = parse_bool(v, cfg.carry_context); }
    if let Some(v) = map.get("permission_mode") { cfg.permission_mode = v.clone(); }
    if let Some(v) = map.get("skip_permissions") { cfg.skip_permissions = parse_bool(v, cfg.skip_permissions); }
    if let Some(v) = map.get("max_turns") { if let Ok(n) = v.trim().parse::<u64>() { cfg.max_turns = n; } }
    if let Some(v) = map.get("allowed_tools") { cfg.allowed_tools = v.clone(); }
    if let Some(v) = map.get("delay_secs") { if let Ok(n) = v.trim().parse::<u64>() { cfg.delay_secs = n; } }
    if let Some(v) = map.get("extra_args") { cfg.extra_args = v.clone(); }
    if let Some(v) = map.get("iter_prompt") { cfg.iter_prompt = v.clone(); }
    Ok(cfg)
}

fn parse_bool(s: &str, default: bool) -> bool { match s.trim() { "true" => true, "false" => false, _ => default } }

pub fn summary(cfg: &LoopConfig) -> String {
    let mut rows: Vec<(String, String)> = Vec::new();
    rows.push(("name".into(), cfg.name.clone()));
    let task_first = cfg.task.lines().next().unwrap_or("").to_string();
    let task_disp = if cfg.task.lines().count() > 1 { format!("{} {}", task_first, ui::dim("(…)")) } else { task_first };
    rows.push(("task".into(), task_disp));
    rows.push(("workdir".into(), cfg.workdir.clone()));
    rows.push(("model".into(), if cfg.model.is_empty() { "default".into() } else { cfg.model.clone() }));
    let stop = match cfg.stop_kind {
        StopKind::Command => format!("command → {}", cfg.stop_command),
        StopKind::Marker => format!("marker → {}", cfg.stop_marker),
        StopKind::Iterations => "fixed iterations".to_string(),
    };
    rows.push(("stop".into(), stop));
    rows.push(("max iters".into(), cfg.max_iterations.to_string()));
    rows.push(("carry context".into(), if cfg.carry_context { "yes" } else { "no" }.into()));
    let perms = if cfg.skip_permissions { ui::red("bypass (--dangerously-skip-permissions)") }
        else if cfg.permission_mode.is_empty() { "default".to_string() } else { cfg.permission_mode.clone() };
    rows.push(("permissions".into(), perms));
    if cfg.max_turns > 0 { rows.push(("max turns".into(), cfg.max_turns.to_string())); }
    if !cfg.allowed_tools.is_empty() { rows.push(("allowed tools".into(), cfg.allowed_tools.clone())); }
    if cfg.delay_secs > 0 { rows.push(("delay".into(), format!("{}s", cfg.delay_secs))); }
    if !cfg.extra_args.is_empty() { rows.push(("extra args".into(), cfg.extra_args.clone())); }
    let width = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    let mut out = String::new();
    for (k, v) in rows {
        let mut key = k;
        while key.chars().count() < width { key.push(' '); }
        out.push_str(&format!("  {}  {}\n", ui::dim(&key), v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_basic() {
        let mut cfg = LoopConfig::default();
        cfg.name = "fix-tests".into();
        cfg.task = "Make the test suite pass.\nFocus on the parser module.".into();
        cfg.stop_kind = StopKind::Command;
        cfg.stop_command = "cargo test".into();
        cfg.max_iterations = 10;
        cfg.carry_context = true;
        cfg.iter_prompt = default_iter_prompt(StopKind::Command);
        let text = to_toml(&cfg);
        let back = from_toml(&text).expect("parse");
        assert_eq!(back.name, cfg.name);
        assert_eq!(back.task, cfg.task);
        assert_eq!(back.stop_kind, cfg.stop_kind);
        assert_eq!(back.stop_command, cfg.stop_command);
        assert_eq!(back.max_iterations, cfg.max_iterations);
        assert_eq!(back.carry_context, cfg.carry_context);
        assert_eq!(back.iter_prompt, cfg.iter_prompt);
    }
    #[test]
    fn escaping_roundtrip() {
        let s = "echo \"hi\" && ls\tpath\\to";
        assert_eq!(unescape_basic(&esc_basic(s)), s);
    }
    #[test]
    fn split_args_quotes() {
        let v = split_args("--foo bar \"two words\" 'single quoted'");
        assert_eq!(v, vec!["--foo", "bar", "two words", "single quoted"]);
    }
    #[test]
    fn args_prompt_last_and_print_first() {
        let mut cfg = LoopConfig::default();
        cfg.model = "sonnet".into();
        let args = claude_args(&cfg, 1, "do the thing");
        assert_eq!(args.first().unwrap(), "--print");
        assert_eq!(args.last().unwrap(), "do the thing");
    }
    #[test]
    fn continue_only_after_first() {
        let cfg = LoopConfig::default();
        assert!(!claude_args(&cfg, 1, "p").contains(&"--continue".to_string()));
        assert!(claude_args(&cfg, 2, "p").contains(&"--continue".to_string()));
    }
}
