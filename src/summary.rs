use std::time::Duration;

use owo_colors::OwoColorize;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

/// Widest the command column may grow
const MAX_CMD_WIDTH: usize = 48;
/// Minimum display width of the header line.
const MIN_HEADER_WIDTH: usize = 30;

/// Outcome of one summary row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success {
        duration: Duration,
    },
    Failure {
        duration: Duration,
        code: Option<i32>,
    },
    NotDetected,
    Declined,
    NotRun,
}

#[derive(Debug, Clone)]
pub struct SummaryRow {
    pub name: String,
    pub cmd: Option<String>,
    pub outcome: Outcome,
}

pub fn should_print(rows: &[SummaryRow]) -> bool {
    rows.iter()
        .filter(|r| !matches!(r.outcome, Outcome::NotDetected))
        .count()
        > 1
}

pub fn render(rows: &[SummaryRow], theme: &Theme) -> String {
    let cells: Vec<Cells> = rows.iter().map(to_cells).collect();
    let name_w = cells.iter().map(|c| c.name.width()).max().unwrap_or(0);
    let cmd_w = cells.iter().map(|c| c.cmd.width()).max().unwrap_or(0);
    let trail_w = cells.iter().map(|c| c.trail.width()).max().unwrap_or(0);

    let row_width = |c: &Cells| {
        let mut w = 2 + name_w + 2;
        if c.trail.is_empty() {
            w += c.cmd.width();
        } else {
            w += cmd_w + 2 + trail_w;
        }
        if let Some(a) = &c.annotation {
            w += 2 + a.width();
        }
        w
    };
    let total = cells
        .iter()
        .map(row_width)
        .max()
        .unwrap_or(0)
        .max(MIN_HEADER_WIDTH);

    let header = format!("─ summary {}", "─".repeat(total.saturating_sub(10)));
    let mut out = header.style(theme.muted).to_string();
    out.push('\n');

    for c in &cells {
        let (status, cmd_style) = match c.row.outcome {
            Outcome::Success { .. } => (theme.success, theme.command),
            Outcome::Failure { .. } => (theme.error, theme.command),
            _ => (theme.muted, theme.muted),
        };
        let mut line = format!(
            "{} {}{}  {}",
            c.glyph.style(status),
            c.name.style(status),
            pad(c.name, name_w),
            c.cmd.style(cmd_style),
        );
        if !c.trail.is_empty() {
            line.push_str(&pad(&c.cmd, cmd_w));
            line.push_str("  ");
            line.push_str(&pad(&c.trail, trail_w));
            line.push_str(&c.trail.style(theme.muted).to_string());
        }
        if let Some(a) = &c.annotation {
            line.push_str("  ");
            line.push_str(&a.style(theme.error).to_string());
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

struct Cells<'a> {
    row: &'a SummaryRow,
    glyph: &'static str,
    name: &'a str,
    cmd: String,
    trail: String,
    annotation: Option<String>,
}

fn to_cells(row: &SummaryRow) -> Cells<'_> {
    let cmd = match (&row.outcome, &row.cmd) {
        (Outcome::NotDetected, _) => "not detected".to_string(),
        (_, Some(cmd)) => truncate_to_width(cmd, MAX_CMD_WIDTH),
        (_, None) => String::new(),
    };
    let (glyph, trail, annotation) = match &row.outcome {
        Outcome::Success { duration } => ("✓", format_duration(*duration), None),
        Outcome::Failure { duration, code } => {
            let annotation = match code {
                Some(code) => format!("exit {code}"),
                None => "signal".to_string(),
            };
            ("✗", format_duration(*duration), Some(annotation))
        }
        Outcome::NotDetected => ("⊘", String::new(), None),
        Outcome::Declined => ("⊘", "declined".to_string(), None),
        Outcome::NotRun => ("⊘", "not run".to_string(), None),
    };
    Cells {
        row,
        glyph,
        name: &row.name,
        cmd,
        trail,
        annotation,
    }
}

fn pad(s: &str, width: usize) -> String {
    " ".repeat(width.saturating_sub(s.width()))
}

/// Cut `s` to at most `max` display columns, ending in `…` when cut.
fn truncate_to_width(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 59_950 {
        // cutoff keeps rounding from producing "60.0s"
        format!("{:.1}s", d.as_secs_f64())
    } else {
        let secs = (ms + 500) / 1000;
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, cmd: Option<&str>, outcome: Outcome) -> SummaryRow {
        SummaryRow {
            name: name.to_string(),
            cmd: cmd.map(str::to_string),
            outcome,
        }
    }

    fn success(ms: u64) -> Outcome {
        Outcome::Success {
            duration: Duration::from_millis(ms),
        }
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn sample_rows() -> Vec<SummaryRow> {
        vec![
            row("format", Some("cargo fmt"), success(200)),
            row("lint", Some("cargo clippy"), success(8_400)),
            row(
                "test",
                Some("cargo test"),
                Outcome::Failure {
                    duration: Duration::from_millis(12_100),
                    code: Some(101),
                },
            ),
            row("typecheck", None, Outcome::NotDetected),
            row("build", Some("cargo build"), Outcome::NotRun),
            row("e2e", Some("pnpm exec playwright test"), Outcome::Declined),
        ]
    }

    #[test]
    fn renders_aligned_block_for_all_outcomes() {
        let expected = "\
─ summary ────────────────────────────────────────────────
✓ format     cargo fmt                      0.2s
✓ lint       cargo clippy                   8.4s
✗ test       cargo test                    12.1s  exit 101
⊘ typecheck  not detected
⊘ build      cargo build                 not run
⊘ e2e        pnpm exec playwright test  declined
";
        assert_eq!(render(&sample_rows(), &Theme::plain()), expected);
    }

    #[test]
    fn no_trailing_whitespace() {
        let rendered = render(&sample_rows(), &Theme::plain());
        for line in rendered.lines() {
            assert_eq!(line, line.trim_end());
        }
    }

    #[test]
    fn styled_output_aligns_like_plain() {
        let rows = sample_rows();
        let styled = render(&rows, &Theme::default());
        assert_eq!(strip_ansi(&styled), render(&rows, &Theme::plain()));
    }

    #[test]
    fn wide_chars_align_with_ascii() {
        let rows = vec![
            row("build", Some("cargo fmt"), success(1_000)),
            row("build", Some("just 构建"), success(1_000)),
        ];
        let rendered = render(&rows, &Theme::plain());
        let lines: Vec<&str> = rendered.lines().skip(1).collect();
        assert!(lines.iter().all(|l| l.ends_with("1.0s")));
        assert_eq!(lines[0].width(), lines[1].width());
    }

    #[test]
    fn truncates_long_commands_with_ellipsis() {
        let long = "x".repeat(60);
        let cut = truncate_to_width(&long, 48);
        assert_eq!(cut.width(), 48);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn truncation_never_splits_wide_chars() {
        // 25 double-width chars = 50 columns; the cut lands mid-character
        let long = "构".repeat(25);
        let cut = truncate_to_width(&long, 48);
        assert_eq!(cut.width(), 47);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn signal_killed_row_says_signal() {
        let rows = vec![
            row("test", Some("cargo test"), success(10)),
            row(
                "build",
                Some("cargo build"),
                Outcome::Failure {
                    duration: Duration::from_millis(10),
                    code: None,
                },
            ),
        ];
        let rendered = render(&rows, &Theme::plain());
        assert!(rendered.contains("signal"));
        assert!(!rendered.contains("exit"));
    }

    #[test]
    fn format_duration_boundaries() {
        let cases = [
            (0, "0.0s"),
            (90, "0.1s"),
            (776, "0.8s"),
            (1_000, "1.0s"),
            (12_100, "12.1s"),
            (59_949, "59.9s"),
            (59_950, "1m 0s"),
            (83_400, "1m 23s"),
            (3_725_000, "62m 5s"),
        ];
        for (ms, expected) in cases {
            assert_eq!(format_duration(Duration::from_millis(ms)), expected);
        }
    }

    #[test]
    fn should_print_needs_more_than_one_subprocess_row() {
        let exec = || row("test", Some("cargo test"), success(10));
        let not_detected = || row("e2e", None, Outcome::NotDetected);

        assert!(!should_print(&[exec()]));
        assert!(should_print(&[exec(), exec()]));
        assert!(!should_print(&[
            exec(),
            not_detected(),
            not_detected(),
            not_detected()
        ]));
        assert!(should_print(&[
            row("doctor", Some("health checks"), success(10)),
            exec()
        ]));
    }
}
