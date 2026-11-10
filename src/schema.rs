//! Describes the settings window: which settings appear on which tab, how
//! each is labelled, and what kind of control edits it. The window itself
//! (`settings.rs`) is built from this, and reads and writes values through
//! `Config::values` and `Config::set`, so it can never disagree with the
//! settings file about what a setting is called or what values it takes.

use crate::config::{Action, ClickAction, IconStyle, LanguageChoice, Position, WarningMonitor};

pub enum Kind {
    /// A tick box.
    Bool,
    /// A whole number, followed by its unit.
    Number(&'static str),
    Text,
    /// A `#RRGGBB` colour, with a button to pick one.
    Color,
    /// A drop-down list of (settings-file name, label) pairs.
    Choice(fn() -> Vec<(&'static str, &'static str)>),
}

pub struct Field {
    /// The setting's name in the settings file.
    pub key: &'static str,
    /// English; translated when shown.
    pub label: &'static str,
    pub kind: Kind,
}

pub struct Tab {
    pub title: &'static str,
    pub fields: &'static [Field],
}

/// Not in the settings file: whether Windows starts screenSleep at sign-in.
pub const AUTOSTART: &str = "autostart";

const fn field(key: &'static str, label: &'static str, kind: Kind) -> Field {
    Field { key, label, kind }
}

pub static TABS: &[Tab] = &[
    Tab {
        title: "General",
        fields: &[
            field("enabled", "Switch screenSleep on", Kind::Bool),
            field("action", "When idle", Kind::Choice(Action::options)),
            field("timeout_minutes", "After", Kind::Number("minutes")),
            field(
                "timeout_minutes_on_battery",
                "On battery, after (0 = the same)",
                Kind::Number("minutes"),
            ),
            field("schedule", "Only between (e.g. 22:00-07:00)", Kind::Text),
            field(
                "block_windows_timeout",
                "Hold off Windows' own screen timeout",
                Kind::Bool,
            ),
            field(
                "screen_off_now_delay_seconds",
                "Delay before \"Turn screen off now\"",
                Kind::Number("seconds"),
            ),
            field("keep_on_minutes", "\"Keep screen on\" lasts", Kind::Number("minutes")),
            field(AUTOSTART, "Start with Windows", Kind::Bool),
            field("language", "Language", Kind::Choice(LanguageChoice::options)),
        ],
    },
    Tab {
        title: "Keep screen on",
        fields: &[
            field("media_keeps_awake", "While music or video is playing", Kind::Bool),
            field("media_min_seconds", "Sound must last at least", Kind::Number("seconds")),
            field(
                "media_ignore_processes",
                "Ignore sound from (e.g. discord.exe)",
                Kind::Text,
            ),
            field(
                "controller_counts_as_input",
                "While a game controller is in use",
                Kind::Bool,
            ),
            field(
                "fullscreen_keeps_awake",
                "While a full-screen app is in front",
                Kind::Bool,
            ),
            field(
                "keep_awake_processes",
                "While these programs run (e.g. vlc.exe)",
                Kind::Text,
            ),
            field(
                "edge_keeps_awake",
                "While the cursor is at a chosen screen edge",
                Kind::Bool,
            ),
            field("keep_awake_edges", "Edges (top, left, bottom, right)", Kind::Text),
            field(
                "wake_on_activity",
                "Turn the screen back on when one of these starts",
                Kind::Bool,
            ),
        ],
    },
    Tab {
        title: "Countdown",
        fields: &[
            field("warning_seconds", "Length (0 = no countdown)", Kind::Number("seconds")),
            field("warning_monitor", "Show on", Kind::Choice(WarningMonitor::options)),
            field("warning_position", "Position", Kind::Choice(Position::options)),
            field("warning_size_percent", "Size", Kind::Number("%")),
            field("warning_opacity_percent", "Opacity", Kind::Number("%")),
            field(
                "warning_dim_percent",
                "Darken the rest of the screen by",
                Kind::Number("%"),
            ),
            field("warning_sound", "Play a sound", Kind::Bool),
            field("warning_fade", "Fade in and out", Kind::Bool),
            field("warning_rounded_corners", "Rounded corners (Windows 11)", Kind::Bool),
            field("warning_title", "Title (empty = automatic)", Kind::Text),
            field("warning_show_hint", "Show the hint below the number", Kind::Bool),
            field("warning_hint", "Hint", Kind::Text),
        ],
    },
    Tab {
        title: "Colors",
        fields: &[
            field("warning_font", "Font", Kind::Text),
            field("warning_background", "Background", Kind::Color),
            field("warning_text_color", "Text", Kind::Color),
            field("warning_number_color", "Number", Kind::Color),
            field("warning_outline", "Pulsing outline", Kind::Bool),
            field("warning_outline_color", "Outline color", Kind::Color),
            field("warning_outline_pulse_color", "Pulses to", Kind::Color),
            field("warning_outline_width_percent", "Outline thickness", Kind::Number("%")),
            field(
                "warning_outline_pulse_ms",
                "One pulse takes (0 = steady)",
                Kind::Number("ms"),
            ),
        ],
    },
    Tab {
        title: "Tray and hotkeys",
        fields: &[
            field("tray_icon", "Tray icon", Kind::Choice(IconStyle::options)),
            field("tray_left_click", "Left-click", Kind::Choice(ClickAction::options)),
            field("tray_middle_click", "Middle-click", Kind::Choice(ClickAction::options)),
            field("notifications", "Show notifications", Kind::Bool),
            field("timeout_menu", "Timeouts in the menu (minutes)", Kind::Text),
            field(
                "hotkey_toggle",
                "Hotkey: switch on or off (e.g. Ctrl+Alt+S)",
                Kind::Text,
            ),
            field("hotkey_screen_off", "Hotkey: turn screen off now", Kind::Text),
            field("hotkey_keep_on", "Hotkey: keep screen on", Kind::Text),
            field("debug_log", "Keep a debug log", Kind::Bool),
        ],
    },
];

/// The most rows on any one tab, which decides how tall the window is.
pub fn max_rows() -> usize {
    TABS.iter().map(|tab| tab.fields.len()).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn fields() -> impl Iterator<Item = &'static Field> {
        TABS.iter().flat_map(|tab| tab.fields).filter(|f| f.key != AUTOSTART)
    }

    #[test]
    fn covers_every_setting_exactly_once() {
        let mut in_window: Vec<&str> = fields().map(|f| f.key).collect();
        let mut in_file: Vec<String> = Config::default().values().into_iter().map(|(key, _)| key).collect();
        in_window.sort_unstable();
        in_file.sort_unstable();
        assert_eq!(in_window, in_file);
    }

    #[test]
    fn every_choice_is_a_value_the_setting_accepts() {
        let mut config = Config::default();
        for f in fields() {
            if let Kind::Choice(options) = f.kind {
                for (value, _) in options() {
                    config.set(f.key, value).unwrap();
                }
            }
        }
    }

    #[test]
    fn kinds_match_the_values() {
        for (key, value) in Config::default().values() {
            let f = fields().find(|f| f.key == key).unwrap();
            let fits = match f.kind {
                Kind::Bool => value == "true" || value == "false",
                Kind::Number(_) => value.parse::<u32>().is_ok(),
                Kind::Color => value.starts_with('#'),
                Kind::Choice(options) => options().iter().any(|(option, _)| *option == value),
                Kind::Text => true,
            };
            assert!(fits, "{key} = {value}");
        }
    }
}
