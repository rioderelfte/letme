pub mod palette;
pub mod style;

use owo_colors::Style;

use crate::config::Config;

/// Semantic theme slots with resolved styles.
pub struct Theme {
    pub success: Style,
    pub error: Style,
    pub warning: Style,
    pub muted: Style,
    pub primary: Style,
    pub accent: Style,
    pub info: Style,
    pub header: Style,
    pub hint: Style,
    pub command: Style,
    pub disabled: Style,
}

impl Theme {
    pub fn load(config: &Config) -> Self {
        if supports_color::on(supports_color::Stream::Stdout).is_none() {
            return Self::plain();
        }
        if let Some(ref name) = config.palette
            && let Some(colors) = palette::load_palette(name)
        {
            return Self::from_palette(&colors);
        }
        Self::default()
    }

    /// Theme with no styling at all, for non-terminal output or NO_COLOR.
    pub(crate) fn plain() -> Self {
        Theme {
            success: Style::new(),
            error: Style::new(),
            warning: Style::new(),
            muted: Style::new(),
            primary: Style::new(),
            accent: Style::new(),
            info: Style::new(),
            header: Style::new(),
            hint: Style::new(),
            command: Style::new(),
            disabled: Style::new(),
        }
    }

    fn from_palette(colors: &std::collections::HashMap<String, String>) -> Self {
        let get = |key: &str, fallback: Style| -> Style {
            colors
                .get(key)
                .map(|s| style::parse_style(s))
                .unwrap_or(fallback)
        };

        let defaults = Self::default();
        Theme {
            success: get("success", defaults.success),
            error: get("error", defaults.error),
            warning: get("warning", defaults.warning),
            muted: get("muted", defaults.muted),
            primary: get("primary", defaults.primary),
            accent: get("accent", defaults.accent),
            info: get("info", defaults.info),
            header: get("header", defaults.header),
            hint: get("hint", defaults.hint),
            command: get("command", defaults.command),
            disabled: get("disabled", defaults.disabled),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            success: Style::new().green(),
            error: Style::new().red(),
            warning: Style::new().yellow(),
            muted: Style::new().bright_black(),
            primary: Style::new().magenta(),
            accent: Style::new().cyan(),
            info: Style::new().blue(),
            header: Style::new().bold().magenta(),
            hint: Style::new().italic().bright_black(),
            command: Style::new().bold(),
            disabled: Style::new().bright_black().strikethrough(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use owo_colors::OwoColorize;

    #[test]
    fn plain_theme_emits_no_ansi_codes() {
        let theme = Theme::plain();
        assert_eq!(format!("{}", "x".style(theme.success)), "x");
        assert_eq!(format!("{}", "x".style(theme.header)), "x");
        assert_eq!(format!("{}", "x".style(theme.disabled)), "x");
    }

    #[test]
    fn default_theme_emits_ansi_codes() {
        let theme = Theme::default();
        assert_ne!(format!("{}", "x".style(theme.success)), "x");
    }
}
