//! Detection of AI coding agents (Claude Code, Codex, etc.) running inside
//! terminal panes, and heuristics for their activity state.
//!
//! The pty thread reports each pane's foreground command line once per second;
//! [`agent_name_from_cmd`] maps that command line to a known agent. The screen
//! thread then derives the agent's activity from the pane's recent output and
//! visible screen content.

use std::time::Duration;

/// An agent that produced output within this window is considered working.
pub const AGENT_WORKING_THRESHOLD: Duration = Duration::from_millis(2500);

/// How far up from the bottom of the viewport to look for a pending
/// confirmation prompt when deciding between Blocked and Idle.
const BLOCKED_SCAN_LINES: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActivity {
    /// The agent is actively producing output.
    Working,
    /// The agent is quiet and its screen shows a confirmation prompt.
    Blocked,
    /// The agent is quiet, waiting for the user's next prompt.
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub name: String,
    pub activity: AgentActivity,
}

/// Binary basenames recognized as AI coding agents.
const KNOWN_AGENTS: &[&str] = &[
    "claude",
    "codex",
    "cursor-agent",
    "opencode",
    "aider",
    "gemini",
    "goose",
    "amp",
    "droid",
    "copilot",
    "crush",
    "grok",
    "qwen",
    "agy",
    "pi",
];

/// Interpreters and launchers that may wrap an agent binary; when argv[0] is
/// one of these, the agent name is searched for in the following arguments.
const WRAPPERS: &[&str] = &[
    "node", "bun", "deno", "python", "python3", "uv", "uvx", "npx", "pnpm", "npm", "yarn", "env",
    "sh", "bash", "zsh", "fish",
];

fn basename(cmd_part: &str) -> &str {
    let base = cmd_part
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(cmd_part)
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".js")
        .trim_end_matches(".mjs");
    base
}

/// Maps a command line (argv) to a known agent name, looking through common
/// interpreter/launcher wrappers (eg. `node /path/to/claude`).
pub fn agent_name_from_cmd(cmd: &[String]) -> Option<String> {
    for (i, part) in cmd.iter().take(4).enumerate() {
        let base = basename(part).to_ascii_lowercase();
        if KNOWN_AGENTS.contains(&base.as_str()) {
            return Some(base);
        }
        if i == 0 && !WRAPPERS.contains(&base.as_str()) {
            // argv[0] is neither an agent nor a wrapper - a shell or an
            // unrelated program owns the terminal's foreground
            return None;
        }
    }
    None
}

/// Phrases that indicate a pending confirmation dialog on the agent's screen.
/// Only consulted when the agent has stopped producing output, so moderately
/// generic phrasing is acceptable.
const BLOCKED_PATTERNS: &[&str] = &[
    "do you want",
    "would you like",
    "(y/n",
    "[y/n",
    "y/n)",
    "yes/no",
    "allow this",
    "allow command",
    "allow once",
    "always allow",
    "allow access",
    "requires approval",
    "waiting for approval",
    "needs your approval",
    "waiting for your",
    "press enter to confirm",
    "proceed?",
];

/// Whether the bottom of a pane's visible screen looks like a confirmation
/// prompt waiting for the user.
pub fn text_indicates_blocked(viewport_text: &str) -> bool {
    viewport_text
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(BLOCKED_SCAN_LINES)
        .any(|line| {
            let line = line.to_ascii_lowercase();
            BLOCKED_PATTERNS.iter().any(|pattern| line.contains(pattern))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_agent_run_directly() {
        assert_eq!(
            agent_name_from_cmd(&cmd(&["claude", "--continue"])),
            Some("claude".into())
        );
        assert_eq!(
            agent_name_from_cmd(&cmd(&["/opt/homebrew/bin/codex"])),
            Some("codex".into())
        );
        assert_eq!(
            agent_name_from_cmd(&cmd(&["pi", "--continue"])),
            Some("pi".into())
        );
    }

    #[test]
    fn detects_agent_under_wrapper() {
        assert_eq!(
            agent_name_from_cmd(&cmd(&["node", "/usr/local/bin/claude"])),
            Some("claude".into())
        );
        assert_eq!(
            agent_name_from_cmd(&cmd(&["npx", "opencode"])),
            Some("opencode".into())
        );
    }

    #[test]
    fn ignores_shells_and_unrelated_programs() {
        assert_eq!(agent_name_from_cmd(&cmd(&["/bin/zsh"])), None);
        assert_eq!(agent_name_from_cmd(&cmd(&["vim", "claude.md"])), None);
        assert_eq!(agent_name_from_cmd(&cmd(&[])), None);
    }

    #[test]
    fn blocked_prompt_detection() {
        assert!(text_indicates_blocked(
            "some output\n\n Do you want to make this edit to main.rs?\n ❯ 1. Yes\n   2. No\n"
        ));
        assert!(!text_indicates_blocked("$ ls\nsrc\nCargo.toml\n$ "));
    }
}
