use owo_colors::{DynColors, Style};

/// Parse a style mini-DSL string into an owo_colors::Style.
///
/// Tokens (space-separated, any order):
/// - modifiers: `bold`, `italic`, `dimmed`, `underline`, `strikethrough`
/// - foreground color: `#RRGGBB` or `fg:#RRGGBB`
/// - background color: `bg:#RRGGBB`
pub fn parse_style(s: &str) -> Style {
    let mut style = Style::new();

    for token in s.split_whitespace() {
        match token {
            "bold" => style = style.bold(),
            "italic" => style = style.italic(),
            "dimmed" => style = style.dimmed(),
            "underline" => style = style.underline(),
            "strikethrough" => style = style.strikethrough(),
            t if t.starts_with("bg:") => {
                if let Some(color) = parse_hex(&t[3..]) {
                    style = style.on_color(color);
                }
            }
            t if t.starts_with("fg:") => {
                if let Some(color) = parse_hex(&t[3..]) {
                    style = style.color(color);
                }
            }
            t if t.starts_with('#') => {
                if let Some(color) = parse_hex(t) {
                    style = style.color(color);
                }
            }
            _ => {} // ignore unknown tokens
        }
    }

    style
}

fn parse_hex(s: &str) -> Option<DynColors> {
    s.parse::<DynColors>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use owo_colors::OwoColorize;

    fn render(spec: &str) -> String {
        format!("{}", "x".style(parse_style(spec)))
    }

    #[test]
    fn empty_spec_leaves_text_unstyled() {
        assert_eq!(render(""), "x");
    }

    #[test]
    fn bare_hex_and_fg_prefix_are_equivalent() {
        assert!(render("#FF0000").contains("38;2;255;0;0"));
        assert_eq!(render("#FF0000"), render("fg:#FF0000"));
    }

    #[test]
    fn bg_prefix_sets_background_color() {
        assert!(render("bg:#10B981").contains("48;2;16;185;129"));
    }

    #[test]
    fn modifiers_combine_with_colors() {
        let out = render("bold fg:#FFFFFF bg:#10B981");
        assert!(out.contains('\u{1b}'), "no ANSI escape at all: {out:?}");
        assert!(out.contains("38;2;255;255;255"), "missing fg: {out:?}");
        assert!(out.contains("48;2;16;185;129"), "missing bg: {out:?}");
        assert!(out != render("fg:#FFFFFF bg:#10B981"), "bold had no effect");
    }

    #[test]
    fn each_modifier_produces_distinct_output() {
        let rendered: Vec<String> = ["bold", "italic", "dimmed", "underline", "strikethrough"]
            .iter()
            .map(|token| render(token))
            .collect();
        for (i, out) in rendered.iter().enumerate() {
            assert_ne!(out, "x", "modifier {i} left the text unstyled");
            assert_eq!(rendered.iter().filter(|o| *o == out).count(), 1);
        }
    }

    #[test]
    fn unknown_tokens_are_ignored() {
        assert_eq!(render("blink #FF0000"), render("#FF0000"));
    }

    #[test]
    fn invalid_hex_is_ignored() {
        assert_eq!(render("#NOTHEX"), "x");
    }
}
