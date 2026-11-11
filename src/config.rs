//! Settings, stored as a commented `key = value` file in
//! `%APPDATA%\screenSleep\settings.ini`.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::hotkey;

pub const MIN_TIMEOUT_MINUTES: u32 = 1;
pub const MAX_TIMEOUT_MINUTES: u32 = 720;
pub const MAX_WARNING_SECONDS: u32 = 300;
pub const MAX_MEDIA_MIN_SECONDS: u32 = 60;

/// Defines an enum whose variants each have a name in the settings file
/// and an (English) label for menus.
macro_rules! setting_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $key:literal, $label:literal);+ $(;)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn key(self) -> &'static str {
                match self { $($name::$variant => $key),+ }
            }

            pub fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            /// Every choice as a (settings-file name, label) pair.
            pub fn options() -> Vec<(&'static str, &'static str)> {
                Self::ALL.iter().map(|v| (v.key(), v.label())).collect()
            }

            fn from_key(key: &str) -> Result<$name, String> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.key().eq_ignore_ascii_case(key))
                    .ok_or_else(|| format!("\"{key}\" should be one of: {}", Self::keys()))
            }

            fn keys() -> String {
                Self::ALL.iter().map(|v| v.key()).collect::<Vec<_>>().join(", ")
            }
        }
    };
}

setting_enum! {
    /// What to do when the idle timeout is reached.
    Action {
        ScreenOff => "screen_off", "Turn screen off";
        Lock => "lock", "Lock";
        LockAndScreenOff => "lock_and_screen_off", "Lock and turn screen off";
        Screensaver => "screensaver", "Start screen saver";
        Sleep => "sleep", "Sleep";
        Hibernate => "hibernate", "Hibernate";
    }
}

impl Action {
    /// The label of the main on/off switch.
    pub fn when_idle_label(self) -> &'static str {
        match self {
            Action::ScreenOff => "Turn screen off when idle",
            Action::Lock => "Lock when idle",
            Action::LockAndScreenOff => "Lock and turn screen off when idle",
            Action::Screensaver => "Start screen saver when idle",
            Action::Sleep => "Sleep when idle",
            Action::Hibernate => "Hibernate when idle",
        }
    }

    /// The line above the number in the countdown.
    pub fn countdown_title(self) -> &'static str {
        match self {
            Action::ScreenOff => "Screen turning off in",
            Action::Lock => "Locking in",
            Action::LockAndScreenOff => "Locking and turning screen off in",
            Action::Screensaver => "Screen saver starting in",
            Action::Sleep => "Going to sleep in",
            Action::Hibernate => "Hibernating in",
        }
    }
}

setting_enum! {
    /// Where on the monitor the countdown appears.
    Position {
        Center => "center", "Center";
        Top => "top", "Top";
        Bottom => "bottom", "Bottom";
        TopLeft => "top_left", "Top left";
        TopRight => "top_right", "Top right";
        BottomLeft => "bottom_left", "Bottom left";
        BottomRight => "bottom_right", "Bottom right";
    }
}

setting_enum! {
    /// Which monitor the countdown appears on.
    WarningMonitor {
        All => "all", "All screens";
        Cursor => "cursor", "The screen the cursor is on";
        Primary => "primary", "The main screen";
    }
}

setting_enum! {
    IconStyle {
        Auto => "auto", "Match taskbar";
        Light => "light", "Light";
        Dark => "dark", "Dark";
    }
}

setting_enum! {
    /// What a click on the tray icon does.
    ClickAction {
        Menu => "menu", "Open the menu";
        Toggle => "toggle", "Switch on or off";
        ScreenOff => "screen_off", "Turn screen off now";
        KeepOn => "keep_on", "Keep screen on";
        Nothing => "none", "Nothing";
    }
}

setting_enum! {
    /// The language of the menus and messages. The labels are each
    /// language's own name, and are not translated.
    LanguageChoice {
        Auto => "auto", "Same as Windows";
        English => "en", "English";
        Spanish => "es", "Español";
        German => "de", "Deutsch";
        French => "fr", "Français";
    }
}

setting_enum! {
    /// Whether Windows' own sounds (alerts, notifications) are left out
    /// when listening for media.
    SystemSounds {
        Auto => "auto", "When the countdown plays a sound";
        Always => "always", "Always";
        Never => "never", "Never";
    }
}

/// The default text under the countdown's number.
pub const DEFAULT_HINT: &str = "Move the mouse or press a key to cancel";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(text: &str) -> Result<Rgb, String> {
        let hex = text.strip_prefix('#').unwrap_or(text);
        let channel = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (hex.len(), channel(0), channel(2), channel(4)) {
            (6, Some(r), Some(g), Some(b)) => Ok(Rgb(r, g, b)),
            _ => Err(format!("\"{text}\" should be a colour like #1A2B3C")),
        }
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

/// The screen edges where a parked cursor keeps the screen on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub top: bool,
    pub left: bool,
    pub bottom: bool,
    pub right: bool,
}

impl Edges {
    fn parse(text: &str) -> Result<Edges, String> {
        let mut edges = Edges::default();
        for name in list_items(text) {
            match name.to_ascii_lowercase().as_str() {
                "top" => edges.top = true,
                "left" => edges.left = true,
                "bottom" => edges.bottom = true,
                "right" => edges.right = true,
                _ => return Err(format!("\"{name}\" should be one of: top, left, bottom, right")),
            }
        }
        Ok(edges)
    }
}

impl fmt::Display for Edges {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let all = [
            (self.top, "top"),
            (self.left, "left"),
            (self.bottom, "bottom"),
            (self.right, "right"),
        ];
        let names: Vec<&str> = all.iter().filter(|(on, _)| *on).map(|&(_, name)| name).collect();
        f.write_str(&names.join(", "))
    }
}

/// A daily time window, e.g. 22:00-07:00. May cross midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// Minutes after midnight.
    pub start: u32,
    pub end: u32,
}

impl Schedule {
    fn parse(text: &str) -> Result<Schedule, String> {
        let time = |t: &str| {
            let (h, m) = t.trim().split_once(':')?;
            let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
            (h * 60 + m <= 24 * 60 && m < 60).then_some(h * 60 + m)
        };
        text.split_once('-')
            .and_then(|(start, end)| {
                Some(Schedule {
                    start: time(start)?,
                    end: time(end)?,
                })
            })
            .ok_or_else(|| format!("\"{text}\" should be a time range like 22:00-07:00"))
    }

    pub fn contains(&self, minute_of_day: u32) -> bool {
        if self.start <= self.end {
            (self.start..self.end).contains(&minute_of_day)
        } else {
            minute_of_day >= self.start || minute_of_day < self.end
        }
    }
}

impl fmt::Display for Schedule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (s, e) = (self.start, self.end);
        write!(f, "{:02}:{:02}-{:02}:{:02}", s / 60, s % 60, e / 60, e % 60)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    // --- General ---
    pub enabled: bool,
    pub action: Action,
    pub timeout_minutes: u32,
    /// Timeout while running on battery; 0 uses `timeout_minutes`.
    pub timeout_minutes_on_battery: u32,
    /// Only act during this window; `None` means always.
    pub schedule: Option<Schedule>,
    /// Hold off Windows' own display timeout while enabled.
    pub block_windows_timeout: bool,
    /// Delay before "Turn screen off now" acts, so that releasing the mouse
    /// button or hotkey doesn't wake the screen straight away.
    pub screen_off_now_delay_seconds: u32,
    /// How long "keep screen on" lasts when started by a click or hotkey.
    pub keep_on_minutes: u32,
    pub language: LanguageChoice,

    // --- Things that keep the screen on ---
    pub media_keeps_awake: bool,
    /// Sound must play continuously for this long before it counts, so that
    /// notification chimes are ignored. 0 makes any sound count.
    pub media_min_seconds: u32,
    /// Sound from these programs (lower-case executable names) is ignored.
    pub media_ignore_processes: Vec<String>,
    pub media_ignore_system_sounds: SystemSounds,
    /// Game controllers count as input, which Windows itself doesn't do.
    pub controller_counts_as_input: bool,
    pub fullscreen_keeps_awake: bool,
    pub edge_keeps_awake: bool,
    pub keep_awake_edges: Edges,
    /// Lower-case executable names, e.g. `vlc.exe`.
    pub keep_awake_processes: Vec<String>,
    /// Turn the screen back on when one of the above starts while it is
    /// off (music starting, a controller being picked up).
    pub wake_on_activity: bool,

    // --- Countdown ---
    /// 0 disables the countdown.
    pub warning_seconds: u32,
    pub warning_sound: bool,
    pub warning_position: Position,
    pub warning_monitor: WarningMonitor,
    pub warning_size_percent: u32,
    pub warning_opacity_percent: u32,
    pub warning_background: Rgb,
    pub warning_text_color: Rgb,
    pub warning_number_color: Rgb,
    pub warning_outline: bool,
    /// The outline fades back and forth between these two colours.
    pub warning_outline_color: Rgb,
    pub warning_outline_pulse_color: Rgb,
    /// Thickness, as a percentage of the countdown's height.
    pub warning_outline_width_percent: u32,
    /// Time for one full pulse; 0 for a steady outline.
    pub warning_outline_pulse_ms: u32,
    /// Fade the countdown in and out rather than have it pop.
    pub warning_fade: bool,
    /// Rounded corners (Windows 11 only).
    pub warning_rounded_corners: bool,
    /// How much to darken the rest of the screen during the countdown.
    pub warning_dim_percent: u32,
    pub warning_font: String,
    /// Empty uses a title that matches `action`.
    pub warning_title: String,
    pub warning_hint: String,
    pub warning_show_hint: bool,

    // --- Tray ---
    pub tray_icon: IconStyle,
    pub tray_left_click: ClickAction,
    pub tray_middle_click: ClickAction,
    pub notifications: bool,
    /// Timeouts offered in the tray menu, in minutes.
    pub timeout_menu: Vec<u32>,

    // --- Hotkeys (empty = none) ---
    pub hotkey_toggle: String,
    pub hotkey_screen_off: String,
    pub hotkey_keep_on: String,

    // --- Troubleshooting ---
    pub debug_log: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            action: Action::ScreenOff,
            timeout_minutes: 15,
            timeout_minutes_on_battery: 0,
            schedule: None,
            block_windows_timeout: true,
            screen_off_now_delay_seconds: 1,
            keep_on_minutes: 60,
            language: LanguageChoice::Auto,

            media_keeps_awake: true,
            media_min_seconds: 2,
            media_ignore_processes: Vec::new(),
            media_ignore_system_sounds: SystemSounds::Auto,
            controller_counts_as_input: true,
            fullscreen_keeps_awake: false,
            edge_keeps_awake: false,
            keep_awake_edges: Edges {
                top: true,
                left: true,
                bottom: false,
                right: false,
            },
            keep_awake_processes: Vec::new(),
            wake_on_activity: true,

            warning_seconds: 30,
            warning_sound: false,
            warning_position: Position::Center,
            warning_monitor: WarningMonitor::All,
            warning_size_percent: 100,
            warning_opacity_percent: 92,
            warning_background: Rgb(0x18, 0x18, 0x1C),
            warning_text_color: Rgb(0xEB, 0xEB, 0xF0),
            warning_number_color: Rgb(0xFF, 0xFF, 0xFF),
            warning_outline: true,
            warning_outline_color: Rgb(0xFF, 0x1F, 0x1F),
            warning_outline_pulse_color: Rgb(0xFF, 0xD6, 0x00),
            warning_outline_width_percent: 3,
            warning_outline_pulse_ms: 1000,
            warning_fade: true,
            warning_rounded_corners: true,
            warning_dim_percent: 30,
            warning_font: "Segoe UI".into(),
            warning_title: String::new(),
            warning_hint: DEFAULT_HINT.into(),
            warning_show_hint: true,

            tray_icon: IconStyle::Auto,
            tray_left_click: ClickAction::Menu,
            tray_middle_click: ClickAction::Toggle,
            notifications: true,
            timeout_menu: vec![1, 2, 5, 10, 15, 20, 30, 45, 60, 120],

            hotkey_toggle: String::new(),
            hotkey_screen_off: String::new(),
            hotkey_keep_on: String::new(),

            debug_log: false,
        }
    }
}

impl Config {
    /// Parses a settings file. Anything that can't be understood keeps its
    /// default and is described in the returned list of problems, so a
    /// damaged file degrades gracefully instead of failing.
    pub fn parse(text: &str) -> (Config, Vec<String>) {
        let text = text.trim_start_matches('\u{feff}');
        if let Some(legacy) = parse_legacy(text) {
            return (legacy, Vec::new());
        }

        let mut config = Config::default();
        let mut problems = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let result = match line.split_once('=') {
                Some((key, value)) => config.set(key.trim().to_ascii_lowercase().as_str(), value.trim()),
                None => Err(format!("\"{line}\" is not a \"setting = value\" line")),
            };
            if let Err(problem) = result {
                problems.push(format!("Line {}: {problem}", number + 1));
            }
        }
        (config, problems)
    }

    /// Changes one setting, given its name and value as they would appear
    /// in the settings file.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "enabled" => self.enabled = boolean(value)?,
            "action" => self.action = Action::from_key(value)?,
            "timeout_minutes" => self.timeout_minutes = number(value, MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES)?,
            "timeout_minutes_on_battery" => self.timeout_minutes_on_battery = number(value, 0, MAX_TIMEOUT_MINUTES)?,
            "schedule" => {
                self.schedule = match value.to_ascii_lowercase().as_str() {
                    "" | "always" => None,
                    _ => Some(Schedule::parse(value)?),
                }
            }
            "block_windows_timeout" => self.block_windows_timeout = boolean(value)?,
            "screen_off_now_delay_seconds" => self.screen_off_now_delay_seconds = number(value, 0, 10)?,
            "keep_on_minutes" => self.keep_on_minutes = number(value, 1, 24 * 60)?,
            "language" => self.language = LanguageChoice::from_key(value)?,

            "media_keeps_awake" => self.media_keeps_awake = boolean(value)?,
            "media_min_seconds" => self.media_min_seconds = number(value, 0, MAX_MEDIA_MIN_SECONDS)?,
            "media_ignore_processes" => {
                self.media_ignore_processes = list_items(value).map(str::to_lowercase).collect();
            }
            "media_ignore_system_sounds" => self.media_ignore_system_sounds = SystemSounds::from_key(value)?,
            "controller_counts_as_input" => self.controller_counts_as_input = boolean(value)?,
            "fullscreen_keeps_awake" => self.fullscreen_keeps_awake = boolean(value)?,
            "edge_keeps_awake" => self.edge_keeps_awake = boolean(value)?,
            "keep_awake_edges" => self.keep_awake_edges = Edges::parse(value)?,
            "keep_awake_processes" => {
                self.keep_awake_processes = list_items(value).map(str::to_lowercase).collect();
            }
            "wake_on_activity" => self.wake_on_activity = boolean(value)?,

            "warning_seconds" => self.warning_seconds = number(value, 0, MAX_WARNING_SECONDS)?,
            "warning_sound" => self.warning_sound = boolean(value)?,
            "warning_position" => self.warning_position = Position::from_key(value)?,
            "warning_monitor" => self.warning_monitor = WarningMonitor::from_key(value)?,
            "warning_size_percent" => self.warning_size_percent = number(value, 40, 300)?,
            "warning_opacity_percent" => self.warning_opacity_percent = number(value, 20, 100)?,
            "warning_background" => self.warning_background = Rgb::parse(value)?,
            "warning_text_color" => self.warning_text_color = Rgb::parse(value)?,
            "warning_number_color" => self.warning_number_color = Rgb::parse(value)?,
            "warning_outline" => self.warning_outline = boolean(value)?,
            "warning_outline_color" => self.warning_outline_color = Rgb::parse(value)?,
            "warning_outline_pulse_color" => self.warning_outline_pulse_color = Rgb::parse(value)?,
            "warning_outline_width_percent" => self.warning_outline_width_percent = number(value, 1, 20)?,
            "warning_outline_pulse_ms" => self.warning_outline_pulse_ms = number(value, 0, 10_000)?,
            "warning_fade" => self.warning_fade = boolean(value)?,
            "warning_rounded_corners" => self.warning_rounded_corners = boolean(value)?,
            "warning_dim_percent" => self.warning_dim_percent = number(value, 0, 90)?,
            "warning_font" => self.warning_font = non_empty(value)?,
            "warning_title" => self.warning_title = value.to_string(),
            "warning_hint" => self.warning_hint = value.to_string(),
            "warning_show_hint" => self.warning_show_hint = boolean(value)?,

            "tray_icon" => self.tray_icon = IconStyle::from_key(value)?,
            "tray_left_click" => self.tray_left_click = ClickAction::from_key(value)?,
            "tray_middle_click" => self.tray_middle_click = ClickAction::from_key(value)?,
            "notifications" => self.notifications = boolean(value)?,
            "timeout_menu" => self.timeout_menu = timeout_list(value)?,

            "hotkey_toggle" => self.hotkey_toggle = hotkey_text(value)?,
            "hotkey_screen_off" => self.hotkey_screen_off = hotkey_text(value)?,
            "hotkey_keep_on" => self.hotkey_keep_on = hotkey_text(value)?,

            "debug_log" => self.debug_log = boolean(value)?,

            _ => return Err(format!("\"{key}\" is not a setting I know")),
        }
        Ok(())
    }

    /// Whether Windows' own sounds are left out when listening for media.
    pub fn ignores_system_sounds(&self) -> bool {
        match self.media_ignore_system_sounds {
            SystemSounds::Always => true,
            SystemSounds::Never => false,
            SystemSounds::Auto => self.warning_sound,
        }
    }

    /// The timeout that applies given the current power source.
    pub fn timeout_minutes_for(&self, on_battery: bool) -> u32 {
        if on_battery && self.timeout_minutes_on_battery > 0 {
            self.timeout_minutes_on_battery
        } else {
            self.timeout_minutes
        }
    }

    pub fn serialize(&self) -> String {
        let mut out = Writer(String::new());
        out.comment("screenSleep settings. Edits are picked up as soon as you save this file.");
        out.comment("Yes/no settings take true or false. Lines starting with # are ignored.");

        out.section("General");
        out.setting("Master switch.", "enabled", self.enabled);
        out.setting(
            &format!(
                "What to do once the PC has been idle long enough. One of: {}.",
                Action::keys()
            ),
            "action",
            self.action.key(),
        );
        out.setting(
            &format!("Minutes of inactivity before that happens ({MIN_TIMEOUT_MINUTES}-{MAX_TIMEOUT_MINUTES})."),
            "timeout_minutes",
            self.timeout_minutes,
        );
        out.setting(
            "A different timeout while running on battery (0 = same as above).",
            "timeout_minutes_on_battery",
            self.timeout_minutes_on_battery,
        );
        out.setting(
            "Only act during these hours, e.g. 22:00-07:00 (24-hour clock; may cross midnight).\nLeave empty to act at any time.",
            "schedule",
            self.schedule.map(|s| s.to_string()).unwrap_or_default(),
        );
        out.setting(
            "Hold off Windows' own display timeout (and screen saver) while enabled, so that\nthe timeout above is the one that applies.",
            "block_windows_timeout",
            self.block_windows_timeout,
        );
        out.setting(
            "Seconds to wait after \"Turn screen off now\" (0-10). Without a short wait, letting\ngo of the mouse button or hotkey wakes the screen again.",
            "screen_off_now_delay_seconds",
            self.screen_off_now_delay_seconds,
        );
        out.setting(
            "How long \"keep screen on\" lasts when started by a tray click or hotkey, in minutes.",
            "keep_on_minutes",
            self.keep_on_minutes,
        );
        out.setting(
            &format!(
                "Language of the menus and messages. One of: {} (auto follows Windows).",
                LanguageChoice::keys()
            ),
            "language",
            self.language.key(),
        );

        out.section("Things that keep the screen on");
        out.setting(
            "Music or video playing (detected by sound output) counts as activity.",
            "media_keeps_awake",
            self.media_keeps_awake,
        );
        out.setting(
            &format!(
                "Sound must play continuously for this many seconds before it counts, so that\nnotification chimes are ignored (0-{MAX_MEDIA_MIN_SECONDS}, 0 = any sound counts)."
            ),
            "media_min_seconds",
            self.media_min_seconds,
        );
        out.setting(
            "Ignore sound from these programs, e.g. discord.exe, so that a voice call doesn't count.",
            "media_ignore_processes",
            self.media_ignore_processes.join(", "),
        );
        out.setting(
            "Ignore Windows' own sounds (alerts, notifications): auto, always or never. With auto\nthey are ignored while the countdown is set to play a sound, since that is one of them.",
            "media_ignore_system_sounds",
            self.media_ignore_system_sounds.key(),
        );
        out.setting(
            "Count game controllers as input. Windows doesn't, so without this the timeout can\nhit in the middle of a game.",
            "controller_counts_as_input",
            self.controller_counts_as_input,
        );
        out.setting(
            "A full-screen app in front (a game, a presentation) counts as activity. Useful for\ngames played with a controller, which Windows doesn't see as input.",
            "fullscreen_keeps_awake",
            self.fullscreen_keeps_awake,
        );
        out.setting(
            "Parking the cursor against an edge of the desktop keeps the screen on.",
            "edge_keeps_awake",
            self.edge_keeps_awake,
        );
        out.setting(
            "Which edges do that. Any of: top, left, bottom, right.",
            "keep_awake_edges",
            self.keep_awake_edges,
        );
        out.setting(
            "Keep the screen on while any of these programs is running, e.g. vlc.exe, obs64.exe.",
            "keep_awake_processes",
            self.keep_awake_processes.join(", "),
        );
        out.setting(
            "Turn the screen back on when one of the above starts while it is off, e.g. when\nmusic starts playing.",
            "wake_on_activity",
            self.wake_on_activity,
        );

        out.section("Countdown");
        out.setting(
            &format!("Length of the countdown shown beforehand (0-{MAX_WARNING_SECONDS}, 0 = no countdown)."),
            "warning_seconds",
            self.warning_seconds,
        );
        out.setting(
            "Play a sound when the countdown appears.",
            "warning_sound",
            self.warning_sound,
        );
        out.setting(
            &format!("One of: {}.", Position::keys()),
            "warning_position",
            self.warning_position.key(),
        );
        out.setting(
            "Which monitors it appears on: all, cursor (the one the cursor is on) or primary.",
            "warning_monitor",
            self.warning_monitor.key(),
        );
        out.setting(
            "Size, as a percentage of normal (40-300).",
            "warning_size_percent",
            self.warning_size_percent,
        );
        out.setting(
            "Opacity (20-100).",
            "warning_opacity_percent",
            self.warning_opacity_percent,
        );
        out.setting("Colours, as #RRGGBB.", "warning_background", self.warning_background);
        out.setting("", "warning_text_color", self.warning_text_color);
        out.setting("", "warning_number_color", self.warning_number_color);
        out.setting(
            "An outline around the countdown that pulses between two colours.",
            "warning_outline",
            self.warning_outline,
        );
        out.setting("", "warning_outline_color", self.warning_outline_color);
        out.setting("", "warning_outline_pulse_color", self.warning_outline_pulse_color);
        out.setting(
            "Outline thickness, as a percentage of the countdown's height (1-20).",
            "warning_outline_width_percent",
            self.warning_outline_width_percent,
        );
        out.setting(
            "Milliseconds for one full pulse (0 = no pulsing, just the outline colour).",
            "warning_outline_pulse_ms",
            self.warning_outline_pulse_ms,
        );
        out.setting("Fade the countdown in and out.", "warning_fade", self.warning_fade);
        out.setting(
            "Rounded corners (Windows 11 only).",
            "warning_rounded_corners",
            self.warning_rounded_corners,
        );
        out.setting(
            "Darken the rest of the screen during the countdown, by this percentage (0-90, 0 = don't).",
            "warning_dim_percent",
            self.warning_dim_percent,
        );
        out.setting("Font name.", "warning_font", &self.warning_font);
        out.setting(
            "Text above the number. Leave empty for one that matches the action.",
            "warning_title",
            &self.warning_title,
        );
        out.setting(
            "Text below the number, and whether to show it.",
            "warning_hint",
            &self.warning_hint,
        );
        out.setting("", "warning_show_hint", self.warning_show_hint);

        out.section("Tray");
        out.setting(
            "Icon colour: auto (matches the taskbar), light or dark.",
            "tray_icon",
            self.tray_icon.key(),
        );
        out.setting(
            &format!(
                "What clicking the icon does. One of: {}.\nRight-click always opens the menu.",
                ClickAction::keys()
            ),
            "tray_left_click",
            self.tray_left_click.key(),
        );
        out.setting("", "tray_middle_click", self.tray_middle_click.key());
        out.setting(
            "Show notifications (first run, hotkey feedback).",
            "notifications",
            self.notifications,
        );
        out.setting(
            "Timeouts offered in the tray menu, in minutes.",
            "timeout_menu",
            self.timeout_menu
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        );

        out.section("Hotkeys");
        out.0.push_str("\r\n");
        out.comment("Global keyboard shortcuts, e.g. Ctrl+Alt+S or Win+Shift+F12. Leave empty for none.");
        out.comment("Modifiers: Ctrl, Alt, Shift, Win. Keys: A-Z, 0-9, F1-F24, Space, Enter, Tab, Esc,");
        out.comment("Insert, Delete, Home, End, PageUp, PageDown, Left, Up, Right, Down, Pause.");
        out.setting("Switch screenSleep on or off.", "hotkey_toggle", &self.hotkey_toggle);
        out.setting("Turn the screen off now.", "hotkey_screen_off", &self.hotkey_screen_off);
        out.setting(
            "Start or stop keeping the screen on.",
            "hotkey_keep_on",
            &self.hotkey_keep_on,
        );

        out.section("Troubleshooting");
        out.setting(
            "Record what screenSleep decides, and why, in screensleep.log next to this file.",
            "debug_log",
            self.debug_log,
        );
        out.0
    }

    /// Every setting's name and current value, as written to the file.
    pub fn values(&self) -> Vec<(String, String)> {
        self.serialize()
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .collect()
    }

    /// Returns `None` if there is no readable settings file.
    pub fn load(path: &Path) -> Option<(Config, Vec<String>)> {
        std::fs::read_to_string(path).ok().map(|t| Config::parse(&t))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write to a temporary file and rename, so a crash mid-write can't
        // leave a truncated settings file behind.
        let tmp = path.with_extension("ini.tmp");
        std::fs::write(&tmp, self.serialize())?;
        std::fs::rename(&tmp, path)
    }
}

/// Builds the text of the settings file.
struct Writer(String);

impl Writer {
    fn comment(&mut self, text: &str) {
        for line in text.lines() {
            self.0.push_str(&format!("# {line}\r\n"));
        }
    }

    fn section(&mut self, title: &str) {
        self.0.push_str(&format!("\r\n\r\n# ===== {title} =====\r\n"));
    }

    /// Writes a setting. One with no comment of its own belongs with the
    /// setting above it.
    fn setting(&mut self, comment: &str, key: &str, value: impl fmt::Display) {
        if !comment.is_empty() {
            self.0.push_str("\r\n");
            self.comment(comment);
        }
        let line = format!("{key} = {value}");
        self.0.push_str(line.trim_end());
        self.0.push_str("\r\n");
    }
}

/// `%APPDATA%\screenSleep\settings.ini`
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("screenSleep").join("settings.ini")
}

/// screenSleep 1.x stored `Enabled,Minutes,StartWithComputer,Minimized`,
/// e.g. `True,15,False,False`. The last two are now tracked by Windows'
/// startup registry entry and no longer apply, respectively.
fn parse_legacy(text: &str) -> Option<Config> {
    let fields: Vec<&str> = text.trim().split(',').map(str::trim).collect();
    if fields.len() != 4 || text.contains('=') {
        return None;
    }
    Some(Config {
        enabled: boolean(fields[0]).ok()?,
        timeout_minutes: number(fields[1], MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES).ok()?,
        ..Config::default()
    })
}

fn boolean(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(format!("\"{value}\" should be true or false")),
    }
}

/// A whole number, pulled into the allowed range if it is outside it.
fn number(value: &str, min: u32, max: u32) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map(|n| n.clamp(min, max))
        .map_err(|_| format!("\"{value}\" should be a whole number from {min} to {max}"))
}

fn non_empty(value: &str) -> Result<String, String> {
    if value.is_empty() {
        Err("this can't be empty".into())
    } else {
        Ok(value.to_string())
    }
}

/// Whether the executable `exe` (lower-case, e.g. `vlc.exe`) is one of
/// `names`. A name matches with or without its `.exe`.
pub fn name_matches(names: &[String], exe: &str) -> bool {
    names
        .iter()
        .any(|name| exe == name || exe.strip_suffix(".exe") == Some(name.as_str()))
}

/// The items of a comma-separated list, ignoring blanks.
fn list_items(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|item| !item.is_empty())
}

fn timeout_list(value: &str) -> Result<Vec<u32>, String> {
    let mut minutes = list_items(value)
        .map(|item| number(item, MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES))
        .collect::<Result<Vec<u32>, String>>()?;
    minutes.sort_unstable();
    minutes.dedup();
    if minutes.is_empty() {
        return Err("list at least one timeout".into());
    }
    Ok(minutes)
}

fn hotkey_text(value: &str) -> Result<String, String> {
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return Ok(String::new());
    }
    hotkey::parse(value).map(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Config {
        Config::parse(text).0
    }

    /// A config in which every setting differs from its default.
    fn customised() -> Config {
        Config {
            enabled: false,
            action: Action::LockAndScreenOff,
            timeout_minutes: 42,
            timeout_minutes_on_battery: 3,
            schedule: Some(Schedule {
                start: 22 * 60,
                end: 7 * 60 + 30,
            }),
            block_windows_timeout: false,
            screen_off_now_delay_seconds: 4,
            keep_on_minutes: 90,
            media_keeps_awake: false,
            media_min_seconds: 5,
            fullscreen_keeps_awake: true,
            edge_keeps_awake: true,
            keep_awake_edges: Edges {
                top: false,
                left: false,
                bottom: true,
                right: true,
            },
            keep_awake_processes: vec!["vlc.exe".into(), "obs64.exe".into()],
            wake_on_activity: false,
            warning_seconds: 10,
            warning_sound: true,
            warning_position: Position::BottomRight,
            warning_monitor: WarningMonitor::Primary,
            warning_size_percent: 60,
            warning_opacity_percent: 75,
            warning_background: Rgb(1, 2, 3),
            warning_text_color: Rgb(0xAA, 0xBB, 0xCC),
            warning_number_color: Rgb(0xFF, 0x80, 0x00),
            warning_outline: false,
            warning_outline_color: Rgb(0x00, 0x80, 0xFF),
            warning_outline_pulse_color: Rgb(0x10, 0x20, 0x30),
            warning_outline_width_percent: 7,
            warning_outline_pulse_ms: 2500,
            warning_font: "Consolas".into(),
            warning_title: "Bye in = #1".into(),
            warning_hint: "Wiggle to stay".into(),
            warning_show_hint: false,
            tray_icon: IconStyle::Light,
            tray_left_click: ClickAction::KeepOn,
            tray_middle_click: ClickAction::ScreenOff,
            notifications: false,
            timeout_menu: vec![3, 7, 90],
            media_ignore_processes: vec!["discord.exe".into()],
            media_ignore_system_sounds: SystemSounds::Never,
            controller_counts_as_input: false,
            debug_log: true,
            language: LanguageChoice::German,
            warning_fade: false,
            warning_rounded_corners: false,
            warning_dim_percent: 55,
            hotkey_toggle: "Ctrl+Alt+T".into(),
            hotkey_screen_off: "Win+Shift+F12".into(),
            hotkey_keep_on: "Pause".into(),
        }
    }

    #[test]
    fn round_trips_without_problems() {
        for config in [Config::default(), customised()] {
            assert_eq!(Config::parse(&config.serialize()), (config, Vec::new()));
        }
    }

    #[test]
    fn imports_v1_settings() {
        let config = parsed("True,45,True,False");
        assert!(config.enabled);
        assert_eq!(config.timeout_minutes, 45);

        let config = parsed("\u{feff}False,15,False,False\r\n");
        assert!(!config.enabled);
        assert_eq!(config.timeout_minutes, 15);
    }

    #[test]
    fn reads_files_written_by_2_0() {
        let (config, problems) = Config::parse(
            "# screenSleep settings.\r\nenabled = false\r\ntimeout_minutes = 20\r\nwarning_seconds = 10\r\n\
             edge_keeps_awake = false\r\nmedia_keeps_awake = true\r\nmedia_min_seconds = 4\r\n",
        );
        assert!(problems.is_empty());
        assert!(!config.enabled && !config.edge_keeps_awake);
        assert_eq!(
            (config.timeout_minutes, config.warning_seconds, config.media_min_seconds),
            (20, 10, 4)
        );
    }

    #[test]
    fn pulls_out_of_range_numbers_into_range() {
        let config = parsed("timeout_minutes = 0\nwarning_seconds = 99999\nmedia_min_seconds = 999");
        assert_eq!(config.timeout_minutes, MIN_TIMEOUT_MINUTES);
        assert_eq!(config.warning_seconds, MAX_WARNING_SECONDS);
        assert_eq!(config.media_min_seconds, MAX_MEDIA_MIN_SECONDS);
        assert_eq!(parsed("True,9999,False,False").timeout_minutes, MAX_TIMEOUT_MINUTES);
    }

    #[test]
    fn reports_problems_and_keeps_defaults() {
        let (config, problems) = Config::parse(
            "; comment\nEnabled = no\ntimeout_minutes = abc\nmystery = 1\naction = explode\n\
             warning_background = red\nhotkey_toggle = Ctrl+Banana\nschedule = late\njunk",
        );
        assert!(!config.enabled);
        assert_eq!(
            Config {
                enabled: true,
                ..config
            },
            Config::default()
        );
        assert_eq!(problems.len(), 7);
        assert!(problems[0].starts_with("Line 3:"));
        assert!(problems[2].contains("screen_off, lock"));

        assert_eq!(Config::parse(""), (Config::default(), Vec::new()));
    }

    #[test]
    fn parses_lists_and_special_values() {
        let config = parsed(
            "keep_awake_processes = VLC.exe, , obs64\nkeep_awake_edges = Bottom,right\n\
             timeout_menu = 30, 5, 5, 9999\nschedule = 9:05-17:00\nhotkey_toggle = none\nwarning_background = a0B1c2",
        );
        assert_eq!(config.keep_awake_processes, ["vlc.exe", "obs64"]);
        assert_eq!(
            config.keep_awake_edges,
            Edges {
                top: false,
                left: false,
                bottom: true,
                right: true
            }
        );
        assert_eq!(config.timeout_menu, [5, 30, 720]);
        assert_eq!(config.schedule, Some(Schedule { start: 545, end: 1020 }));
        assert_eq!(config.hotkey_toggle, "");
        assert_eq!(config.warning_background, Rgb(0xA0, 0xB1, 0xC2));
        assert_eq!(parsed("schedule = 22:00-07:00\nschedule = always").schedule, None);
    }

    #[test]
    fn program_names_match_with_or_without_exe() {
        let names = ["vlc.exe".to_string(), "obs64".to_string()];
        assert!(name_matches(&names, "vlc.exe") && name_matches(&names, "obs64.exe") && name_matches(&names, "obs64"));
        assert!(!name_matches(&names, "vlc") && !name_matches(&names, "notvlc.exe") && !name_matches(&[], "vlc.exe"));
    }

    #[test]
    fn values_can_be_set_back() {
        let source = customised();
        let mut copy = Config::default();
        for (key, value) in source.values() {
            copy.set(&key, &value).unwrap();
        }
        assert_eq!(copy, source);
    }

    #[test]
    fn schedule_windows() {
        let day = Schedule {
            start: 9 * 60,
            end: 17 * 60,
        };
        assert!(day.contains(9 * 60) && day.contains(16 * 60 + 59));
        assert!(!day.contains(17 * 60) && !day.contains(8 * 60 + 59));

        let night = Schedule {
            start: 22 * 60,
            end: 7 * 60,
        };
        assert!(night.contains(23 * 60) && night.contains(0) && night.contains(6 * 60 + 59));
        assert!(!night.contains(7 * 60) && !night.contains(12 * 60));

        assert!(Schedule::parse("25:00-07:00").is_err());
        assert!(Schedule::parse("22:60-07:00").is_err());
        assert_eq!(Schedule::parse("00:00-24:00").unwrap().to_string(), "00:00-24:00");
    }

    #[test]
    fn system_sounds_follow_the_countdown_sound_by_default() {
        let config = |sound, choice| Config {
            warning_sound: sound,
            media_ignore_system_sounds: choice,
            ..Config::default()
        };
        assert!(config(true, SystemSounds::Auto).ignores_system_sounds());
        assert!(!config(false, SystemSounds::Auto).ignores_system_sounds());
        assert!(config(false, SystemSounds::Always).ignores_system_sounds());
        assert!(!config(true, SystemSounds::Never).ignores_system_sounds());
    }

    #[test]
    fn battery_timeout() {
        let config = Config {
            timeout_minutes: 15,
            timeout_minutes_on_battery: 5,
            ..Config::default()
        };
        assert_eq!(config.timeout_minutes_for(false), 15);
        assert_eq!(config.timeout_minutes_for(true), 5);
        assert_eq!(Config::default().timeout_minutes_for(true), 15);
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("screensleep-test-{}", std::process::id()));
        let path = dir.join("nested").join("settings.ini");
        assert_eq!(Config::load(&path), None);
        let config = customised();
        config.save(&path).unwrap();
        config.save(&path).unwrap(); // overwriting an existing file works
        assert_eq!(Config::load(&path), Some((config, Vec::new())));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
