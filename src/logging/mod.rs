//! Terminal output, brand palette, and theme utilities.
//!
//! Ports `reference/color.ts` and `reference/theme.ts` to idiomatic Rust
//! using the `colored` crate. Single source of truth for all CLI styling.
//!
//! Many functions here are not yet called in Phase 0 but will be wired up
//! in subsequent phases when command implementations land.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use colored::{ColoredString, Colorize};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------------------
// Quiet mode
// ---------------------------------------------------------------------------

static QUIET: AtomicBool = AtomicBool::new(false);

/// Enable or disable quiet mode (suppresses non-error output).
pub fn set_quiet(enabled: bool) {
    QUIET.store(enabled, Ordering::Relaxed);
}

/// Returns true if quiet mode is active.
pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Brand palette  (os-eco brand colors)
// ---------------------------------------------------------------------------

/// Forest green — Overstory primary brand color. RGB(46, 125, 50)
pub fn brand(text: &str) -> ColoredString {
    text.truecolor(46, 125, 50)
}

/// Forest green bold variant.
pub fn brand_bold(text: &str) -> ColoredString {
    text.truecolor(46, 125, 50).bold()
}

/// Amber — highlights, warnings. RGB(255, 183, 77)
pub fn accent(text: &str) -> ColoredString {
    text.truecolor(255, 183, 77)
}

/// Stone gray — secondary text, muted content. RGB(120, 120, 110)
pub fn muted(text: &str) -> ColoredString {
    text.truecolor(120, 120, 110)
}

// ---------------------------------------------------------------------------
// Standard color helpers
// ---------------------------------------------------------------------------

pub fn color_bold(text: &str) -> ColoredString {
    text.bold()
}

pub fn color_dim(text: &str) -> ColoredString {
    text.dimmed()
}

pub fn color_red(text: &str) -> ColoredString {
    text.red()
}

pub fn color_green(text: &str) -> ColoredString {
    text.green()
}

pub fn color_yellow(text: &str) -> ColoredString {
    text.yellow()
}

pub fn color_blue(text: &str) -> ColoredString {
    text.blue()
}

pub fn color_magenta(text: &str) -> ColoredString {
    text.magenta()
}

pub fn color_cyan(text: &str) -> ColoredString {
    text.cyan()
}

pub fn color_gray(text: &str) -> ColoredString {
    text.bright_black()
}

// ---------------------------------------------------------------------------
// Standardized message formatters
// ---------------------------------------------------------------------------

/// Success: brand checkmark + brand message. Optional accent-colored ID.
pub fn print_success(msg: &str, id: Option<&str>) {
    if is_quiet() {
        return;
    }
    let id_part = id.map(|i| format!(" {}", accent(i))).unwrap_or_default();
    println!("{} {}{}", brand_bold("✓"), brand(msg), id_part);
}

/// Warning: yellow ! + yellow message. Optional dim hint.
pub fn print_warning(msg: &str, hint: Option<&str>) {
    if is_quiet() {
        return;
    }
    let hint_part = hint
        .map(|h| format!(" {}", color_dim(&format!("— {h}"))))
        .unwrap_or_default();
    println!("{} {}{}", "!".yellow().bold(), msg.yellow(), hint_part);
}

/// Error: red ✗ + red message. Optional dim hint. Always to stderr.
pub fn print_error(msg: &str, hint: Option<&str>) {
    let hint_part = hint
        .map(|h| format!(" {}", color_dim(&format!("— {h}"))))
        .unwrap_or_default();
    eprintln!("{} {}{}", "✗".red().bold(), msg.red(), hint_part);
}

/// Hint/info: dim indented text.
pub fn print_hint(msg: &str) {
    if is_quiet() {
        return;
    }
    println!("{}", color_dim(&format!("  {msg}")));
}

// ---------------------------------------------------------------------------
// ANSI strip utilities
// ---------------------------------------------------------------------------

/// Strip ANSI escape codes from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for inner in chars.by_ref() {
                if inner == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Visible string length (excluding ANSI escape codes).
pub fn visible_length(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

// ---------------------------------------------------------------------------
// Duration and relative time formatting
// ---------------------------------------------------------------------------

/// Format a duration in milliseconds to a human-readable string.
///
/// Examples: `"42ms"`, `"1.4s"`, `"3.2m"`, `"1.1h"`
pub fn format_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

/// Format an ISO 8601 timestamp as a relative time string (e.g. `"2m ago"`).
///
/// Returns the original string if it cannot be parsed.
pub fn format_relative_time(timestamp: &str) -> String {
    match DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt) => {
            let now = Utc::now();
            let diff = now.signed_duration_since(dt.with_timezone(&Utc));
            let secs = diff.num_seconds();
            if secs < 0 {
                "just now".to_string()
            } else if secs < 60 {
                format!("{secs}s ago")
            } else if secs < 3_600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86_400 {
                format!("{}h ago", secs / 3_600)
            } else {
                format!("{}d ago", secs / 86_400)
            }
        }
        Err(_) => timestamp.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Agent state theme
// ---------------------------------------------------------------------------

/// Returns a colored string for an agent state icon.
///
/// States: `working` (green >), `booting` (yellow ~), `stalled` (red !),
/// `zombie` (gray x), `completed` (cyan ✓). Unknown states get dim `?`.
pub fn state_icon_colored(state: &str) -> ColoredString {
    match state {
        "working" => ">".green(),
        "booting" => "~".yellow(),
        "stalled" => "!".red(),
        "zombie" => "x".bright_black(),
        "completed" => "✓".cyan(),
        _ => "?".dimmed(),
    }
}

/// Returns the raw icon character for an agent state.
pub fn state_icon(state: &str) -> &'static str {
    match state {
        "working" => ">",
        "booting" => "~",
        "stalled" => "!",
        "zombie" => "x",
        "completed" => "✓",
        _ => "?",
    }
}

// ---------------------------------------------------------------------------
// Event label theme
// ---------------------------------------------------------------------------

/// Label data for an event type.
pub struct EventLabel {
    /// 5-character compact label (for feed).
    pub compact: &'static str,
    /// 10-character full label (for trace/replay).
    pub full: &'static str,
    /// ANSI color name string.
    pub color: fn(&str) -> ColoredString,
}

/// Returns the [`EventLabel`] for a given event type string.
pub fn event_label(event_type: &str) -> EventLabel {
    match event_type {
        "tool_start" => EventLabel {
            compact: "TOOL+",
            full: "TOOL START",
            color: color_blue,
        },
        "tool_end" => EventLabel {
            compact: "TOOL-",
            full: "TOOL END  ",
            color: color_blue,
        },
        "session_start" => EventLabel {
            compact: "SESS+",
            full: "SESSION  +",
            color: color_green,
        },
        "session_end" => EventLabel {
            compact: "SESS-",
            full: "SESSION  -",
            color: color_yellow,
        },
        "mail_sent" => EventLabel {
            compact: "MAIL>",
            full: "MAIL SENT ",
            color: color_cyan,
        },
        "mail_received" => EventLabel {
            compact: "MAIL<",
            full: "MAIL RECV ",
            color: color_cyan,
        },
        "spawn" => EventLabel {
            compact: "SPAWN",
            full: "SPAWN     ",
            color: color_magenta,
        },
        "error" => EventLabel {
            compact: "ERROR",
            full: "ERROR     ",
            color: color_red,
        },
        "custom" => EventLabel {
            compact: "CUSTM",
            full: "CUSTOM    ",
            color: color_gray,
        },
        "turn_start" => EventLabel {
            compact: "TURN+",
            full: "TURN START",
            color: color_green,
        },
        "turn_end" => EventLabel {
            compact: "TURN-",
            full: "TURN END  ",
            color: color_yellow,
        },
        "progress" => EventLabel {
            compact: "PROG ",
            full: "PROGRESS  ",
            color: color_cyan,
        },
        "result" => EventLabel {
            compact: "RSULT",
            full: "RESULT    ",
            color: color_green,
        },
        _ => EventLabel {
            compact: "?????",
            full: "UNKNOWN   ",
            color: color_dim,
        },
    }
}

// ---------------------------------------------------------------------------
// Separators and headers
// ---------------------------------------------------------------------------

/// Unicode thin horizontal box-drawing character.
pub const SEPARATOR_CHAR: char = '\u{2500}';

/// Unicode double horizontal box-drawing character (thick).
pub const THICK_SEPARATOR_CHAR: char = '\u{2550}';

/// Default line width for separators and headers.
pub const DEFAULT_WIDTH: usize = 70;

/// Returns a thin separator line of the given width (default 70).
pub fn separator(width: Option<usize>) -> String {
    SEPARATOR_CHAR
        .to_string()
        .repeat(width.unwrap_or(DEFAULT_WIDTH))
}

/// Returns a thick (double-line) separator of the given width (default 70).
pub fn thick_separator(width: Option<usize>) -> String {
    THICK_SEPARATOR_CHAR
        .to_string()
        .repeat(width.unwrap_or(DEFAULT_WIDTH))
}

/// Pads a string to the given visible width, accounting for ANSI escape codes.
pub fn pad_visible(s: &str, width: usize) -> String {
    let visible = visible_length(s);
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Renders a primary header: brand bold title + newline + thin separator.
pub fn render_header(title: &str, width: Option<usize>) -> String {
    format!("{}\n{}", brand_bold(title), separator(width))
}

/// Renders a secondary header: bold title + newline + dim thin separator.
pub fn render_sub_header(title: &str, width: Option<usize>) -> String {
    format!("{}\n{}", color_bold(title), color_dim(&separator(width)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_plain() {
        assert_eq!(strip_ansi("hello"), "hello");
    }

    #[test]
    fn test_strip_ansi_with_codes() {
        let s = "\x1b[32mhello\x1b[0m";
        assert_eq!(strip_ansi(s), "hello");
    }

    #[test]
    fn test_visible_length() {
        let s = "\x1b[32mhello\x1b[0m";
        assert_eq!(visible_length(s), 5);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(42), "42ms");
        assert_eq!(format_duration(1500), "1.5s");
        assert_eq!(format_duration(90_000), "1.5m");
        assert_eq!(format_duration(3_600_000 * 2), "2.0h");
    }

    #[test]
    fn test_separator_length() {
        assert_eq!(separator(None).chars().count(), DEFAULT_WIDTH);
        assert_eq!(separator(Some(10)).chars().count(), 10);
    }

    #[test]
    fn test_pad_visible() {
        let s = "hello";
        assert_eq!(pad_visible(s, 10), "hello     ");
        assert_eq!(pad_visible(s, 3), "hello"); // no truncation
    }

    #[test]
    fn test_state_icon() {
        assert_eq!(state_icon("working"), ">");
        assert_eq!(state_icon("booting"), "~");
        assert_eq!(state_icon("stalled"), "!");
        assert_eq!(state_icon("zombie"), "x");
        assert_eq!(state_icon("completed"), "✓");
        assert_eq!(state_icon("unknown"), "?");
    }

    #[test]
    fn test_quiet_mode() {
        set_quiet(false);
        assert!(!is_quiet());
        set_quiet(true);
        assert!(is_quiet());
        set_quiet(false); // reset
    }

    #[test]
    fn test_format_relative_time_invalid() {
        assert_eq!(format_relative_time("not-a-date"), "not-a-date");
    }
}
