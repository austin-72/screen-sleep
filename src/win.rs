//! The Windows side: tray icon, menu, countdown overlay and the idle timer.
//!
//! Everything runs on one thread, driven by a window timer, so there is no
//! shared state to synchronise and nothing to abort.

use std::cell::{Cell, OnceCell, RefCell};
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant, SystemTime};

use windows_sys::core::{w, BOOL, PCWSTR};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Input::XboxController::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{self, Action, ClickAction, Config, IconStyle, Position, Rgb, WarningMonitor};
use crate::hotkey;
use crate::lang::{self, minutes_label, seconds_label, t, tf};
use crate::media::AudioMonitor;
use crate::policy::{self, Phase, SoundTracker, Timing};
use crate::settings;

const APP_NAME: &str = "screenSleep";
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("screenSleep");
const THEME_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
const THEME_VALUE: PCWSTR = w!("SystemUsesLightTheme");

const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

const TIMER_TICK: usize = 1;
const TIMER_OFF_NOW: usize = 2;
const TIMER_PULSE: usize = 3;
/// How often the countdown's outline is redrawn while it pulses.
const PULSE_FRAME_MS: u32 = 33;
const TIMER_FADE: usize = 4;
const FADE_FRAME_MS: u32 = 16;
/// How long the countdown takes to fade in or out.
const FADE_MS: f32 = 180.0;
/// How often idle time is checked normally, and the faster rate used while
/// the countdown is up (so it disappears the moment the user comes back) or
/// while deciding whether a sound is a brief chime or sustained media.
const TICK_MS: u32 = 1000;
const TICK_FAST_MS: u32 = 200;
/// Looking through the list of running programs is comparatively costly, so
/// it is done at most this often.
const PROCESS_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const PREVIEW_DURATION: Duration = Duration::from_secs(10);

const SC_MONITORPOWER_OFF: LPARAM = 2;

const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_SCREEN_OFF: i32 = 2;
const HOTKEY_KEEP_ON: i32 = 3;

// Menu commands. The `_BASE` ones cover a range: base + index or value.
const CMD_ENABLED: usize = 1;
const CMD_OFF_NOW: usize = 2;
const CMD_EDGE: usize = 3;
const CMD_AUTOSTART: usize = 4;
const CMD_SETTINGS: usize = 5;
const CMD_EXIT: usize = 6;
const CMD_MEDIA: usize = 7;
const CMD_FULLSCREEN: usize = 8;
const CMD_WARNING_SOUND: usize = 9;
const CMD_KEEP_ON_STOP: usize = 10;
const CMD_PREVIEW: usize = 11;
const CMD_ACTION_BASE: usize = 100;
const CMD_ICON_BASE: usize = 120;
const CMD_POSITION_BASE: usize = 140;
const CMD_MONITOR_BASE: usize = 160;
const CMD_WARNING_BASE: usize = 1000;
const CMD_KEEP_ON_BASE: usize = 2000;
const CMD_TIMEOUT_BASE: usize = 4000;

const WARNING_CHOICES: [u32; 6] = [5, 10, 15, 30, 60, 120];
const KEEP_ON_CHOICES: [u32; 6] = [15, 30, 60, 120, 240, 480];

/// Something that is keeping the screen on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    KeepOnTimer,
    Media,
    Controller,
    Edge,
    Fullscreen,
    Program,
}

impl Hold {
    fn reason(self) -> &'static str {
        match self {
            Hold::KeepOnTimer => t("\"keep screen on\" timer"),
            Hold::Media => t("music or video playing"),
            Hold::Controller => t("game controller in use"),
            Hold::Edge => t("cursor at a screen edge"),
            Hold::Fullscreen => t("full-screen app in front"),
            Hold::Program => t("a listed program running"),
        }
    }
}

pub(crate) struct App {
    hwnd: HWND,
    hinstance: HINSTANCE,
    /// Countdown windows, one per monitor it is shown on. Created as needed
    /// and then reused.
    overlays: RefCell<Vec<HWND>>,
    /// Windows that darken the rest of each screen behind the countdown.
    dims: RefCell<Vec<HWND>>,
    /// How far the countdown has faded in: 0.0 hidden, 1.0 fully shown.
    fade_level: Cell<f32>,
    fade_target: Cell<f32>,
    /// Whether the fade animation timer is running.
    fading: Cell<bool>,
    /// Dark glyph, for light taskbars.
    icon_dark: HICON,
    /// Light glyph, for dark taskbars.
    icon_light: HICON,
    /// `None` if the audio system couldn't be reached; media is then ignored.
    audio: Option<AudioMonitor>,
    /// Reference point for the timestamps fed to `sound`.
    started: Instant,
    sound: Cell<SoundTracker>,
    /// When something other than keyboard or mouse input last counted as
    /// activity (media playing, a keep-awake condition).
    last_activity: Cell<Option<Instant>>,
    /// "Keep screen on for ..." is in effect until then.
    keep_on_until: Cell<Option<Instant>>,
    /// When the running programs were last checked, and the result.
    process_check: Cell<Option<(Instant, bool)>>,
    preview_until: Cell<Option<Instant>>,
    /// The number the countdown should be showing, if it should be up.
    overlay_seconds: Cell<Option<u32>>,
    /// The number last painted; still wanted while the countdown fades out.
    overlay_number: Cell<u32>,
    taskbar_created: u32,
    config_path: PathBuf,
    config: RefCell<Config>,
    config_mtime: Cell<Option<SystemTime>>,
    phase: Cell<Phase>,
    display_required: Cell<bool>,
    tick_ms: Cell<u32>,
    /// What is keeping the screen on right now, if anything.
    hold: Cell<Option<Hold>>,
    /// The most recent thing that did, and the time of day (in minutes) it
    /// was last seen, to answer "why was my screen still on?".
    last_hold: Cell<Option<(Hold, u32)>>,
    /// Whether the outline animation timer is running.
    pulsing: Cell<bool>,
}

thread_local! {
    static APP: OnceCell<App> = const { OnceCell::new() };
}

/// Runs `f` with the app state, if it has been initialised yet.
pub(crate) fn with_app<R>(f: impl FnOnce(&App) -> R) -> Option<R> {
    APP.with(|app| app.get().map(f))
}

pub fn run() {
    unsafe {
        // Only one copy at a time; a second launch just exits.
        CreateMutexW(null(), 0, w!("Local\\screenSleep.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(null());
        // Icon resources 1 and 2 (see assets/app.rc), at the size the tray wants.
        let load_icon = |id: usize| {
            let (width, height) = (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
            let icon = LoadImageW(hinstance, id as PCWSTR, IMAGE_ICON, width, height, LR_SHARED);
            if icon.is_null() {
                LoadIconW(null_mut(), IDI_APPLICATION)
            } else {
                icon
            }
        };

        let mut class: WNDCLASSW = zeroed();
        class.hInstance = hinstance;
        class.lpszClassName = w!("screenSleep.Main");
        class.lpfnWndProc = Some(main_proc);
        RegisterClassW(&class);

        class.lpszClassName = w!("screenSleep.Overlay");
        class.lpfnWndProc = Some(overlay_proc);
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        RegisterClassW(&class);

        class.lpszClassName = w!("screenSleep.Dim");
        class.lpfnWndProc = Some(DefWindowProcW);
        class.hbrBackground = GetStockObject(BLACK_BRUSH);
        RegisterClassW(&class);

        // A hidden top-level window (not a message-only one, which would miss
        // the "TaskbarCreated" broadcast) receives tray and timer messages.
        let hwnd = CreateWindowExW(
            0,
            w!("screenSleep.Main"),
            w!("screenSleep"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }

        let config_path = config::default_path();
        let loaded = Config::load(&config_path);
        let first_run = loaded.is_none();
        let (config, problems) = loaded.unwrap_or_default();
        lang::set_language(lang::resolve(config.language, GetUserDefaultUILanguage()));

        let app = App {
            hwnd,
            hinstance,
            overlays: RefCell::new(Vec::new()),
            dims: RefCell::new(Vec::new()),
            fade_level: Cell::new(0.0),
            fade_target: Cell::new(0.0),
            fading: Cell::new(false),
            icon_dark: load_icon(1),
            icon_light: load_icon(2),
            audio: AudioMonitor::new(),
            started: Instant::now(),
            sound: Cell::new(SoundTracker::Silent),
            last_activity: Cell::new(None),
            keep_on_until: Cell::new(None),
            process_check: Cell::new(None),
            preview_until: Cell::new(None),
            overlay_seconds: Cell::new(None),
            overlay_number: Cell::new(0),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            config_path,
            config: RefCell::new(config),
            config_mtime: Cell::new(None),
            phase: Cell::new(Phase::Active),
            display_required: Cell::new(false),
            tick_ms: Cell::new(TICK_MS),
            hold: Cell::new(None),
            last_hold: Cell::new(None),
            pulsing: Cell::new(false),
        };
        // Rewrite the file on startup: creates it on first run and brings a
        // file from an older version up to date with every current setting.
        app.save_config();
        app.tray(NIM_ADD);
        if !problems.is_empty() {
            app.report_problems(&problems);
        } else if first_run {
            app.notify(t(
                "screenSleep is running here in the tray. Right-click the icon for options.",
            ));
        }
        app.register_hotkeys();
        app.log(&format!("screenSleep {} started", env!("CARGO_PKG_VERSION")));
        // If the .exe has moved since autostart was turned on (or autostart
        // still points at a 1.x copy), point it at this one.
        if autostart_command().is_some_and(|cmd| cmd != expected_autostart_command()) {
            set_autostart(true);
        }
        app.tick();
        SetTimer(hwnd, TIMER_TICK, TICK_MS, None);
        APP.with(|cell| {
            let _ = cell.set(app);
        });

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            // Gives the settings window its keyboard handling (Tab, Enter, Esc).
            let dialog = settings::window();
            if !dialog.is_null() && IsDialogMessageW(dialog, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        SetThreadExecutionState(ES_CONTINUOUS);
    }
}

impl App {
    pub(crate) fn config(&self) -> Config {
        self.config.borrow().clone()
    }

    /// The heart of the app: works out how long it has been since any
    /// activity and shows the countdown / acts accordingly.
    fn tick(&self) {
        self.reload_config_if_changed();
        let config = self.config();
        let now = Instant::now();

        let keeping_on = match self.keep_on_until.get() {
            Some(until) if now < until => true,
            Some(_) => {
                self.keep_on_until.set(None);
                self.tray(NIM_MODIFY);
                false
            }
            None => false,
        };
        let enabled = config.enabled && config.schedule.is_none_or(|s| s.contains(local_minute_of_day()));

        // Anything that keeps the screen on counts as activity, exactly like
        // touching the mouse: when it ends, the full timeout starts over.
        let media = self.media_playing(enabled && config.media_keeps_awake, &config);
        let hold = if keeping_on {
            Some(Hold::KeepOnTimer)
        } else if media {
            Some(Hold::Media)
        } else if enabled {
            self.held_awake(&config, now)
        } else {
            None
        };
        let held = hold.is_some();
        if let Some(hold) = hold {
            self.last_activity.set(Some(now));
            self.last_hold.set(Some((hold, local_minute_of_day())));
        }
        if self.hold.replace(hold) != hold {
            self.log(&match hold {
                Some(hold) => format!("Keeping the screen on: {}", hold.reason()),
                None => "Nothing is keeping the screen on any more".to_string(),
            });
        }

        let timing = Timing {
            enabled,
            timeout_ms: u64::from(config.timeout_minutes_for(on_battery())) * 60_000,
            warning_ms: u64::from(config.warning_seconds) * 1_000,
        };
        let since_activity = self
            .last_activity
            .get()
            .map(|at| now.duration_since(at).as_millis() as u64);
        let idle_ms = policy::effective_idle_ms(input_idle_ms(), since_activity);

        let old = self.phase.get();
        let mut new = policy::phase(&timing, idle_ms);
        if old == Phase::Active && new == Phase::Off && timing.warning_ms > 0 {
            // We'd be acting without ever having shown the countdown. That
            // happens when the rules change under a long-idle PC: a shorter
            // timeout, the schedule starting, waking from sleep. Start the
            // clock again instead, so there is always fair warning.
            self.last_activity.set(Some(now));
            new = Phase::Active;
        }
        self.phase.set(new);
        if std::mem::discriminant(&old) != std::mem::discriminant(&new) {
            self.log(&format!(
                "{old:?} -> {new:?} after {} s without activity",
                idle_ms / 1000
            ));
        }

        // While we're in charge of the timeout, stop Windows' own display
        // timer from getting in first. Let go of that during the countdown
        // and once we've acted, and whenever we're disabled.
        let in_charge = enabled && new == Phase::Active && (config.block_windows_timeout || held);
        self.require_display(keeping_on || in_charge);

        let counting_down = matches!(new, Phase::Warning { .. });
        if counting_down && !matches!(old, Phase::Warning { .. }) && config.warning_sound {
            unsafe { MessageBeep(MB_ICONWARNING) };
        }
        // Only on the transition, so we don't act again if something else
        // wakes the screen without user input.
        if new == Phase::Off && old != Phase::Off {
            self.perform(config.action);
        }

        let previewing = self.preview_until.get().is_some_and(|until| now < until);
        let seconds = match (new, self.preview_until.get()) {
            (Phase::Warning { seconds_left }, _) => Some(seconds_left),
            (_, Some(until)) if previewing => Some(until.duration_since(now).as_secs() as u32 + 1),
            _ => None,
        };
        self.set_overlay(seconds, &config);

        let fast = counting_down || previewing || self.sound.get().needs_fast_sampling();
        self.set_tick_interval(if fast { TICK_FAST_MS } else { TICK_MS });
    }

    /// Whether sustained sound (as opposed to a notification chime) is
    /// coming out of the speakers.
    fn media_playing(&self, watch: bool, config: &Config) -> bool {
        if !watch {
            self.sound.set(SoundTracker::Silent);
            return false;
        }
        let audible = self
            .audio
            .as_ref()
            .is_some_and(|audio| audio.is_audible(&config.media_ignore_processes));
        let now_ms = self.started.elapsed().as_millis() as u64;
        let min_ms = u64::from(config.media_min_seconds) * 1000;
        let sound = self.sound.get().update(now_ms, audible, min_ms);
        self.sound.set(sound);
        sound.is_playing()
    }

    /// Which of the optional keep-awake conditions currently holds, if any.
    fn held_awake(&self, config: &Config, now: Instant) -> Option<Hold> {
        if config.controller_counts_as_input && controller_in_use() {
            return Some(Hold::Controller);
        }
        if config.edge_keeps_awake && cursor_on_edge(config) {
            return Some(Hold::Edge);
        }
        if config.fullscreen_keeps_awake && fullscreen_app_in_front() {
            return Some(Hold::Fullscreen);
        }
        if config.keep_awake_processes.is_empty() {
            return None;
        }
        let running = match self.process_check.get() {
            Some((at, running)) if now.duration_since(at) < PROCESS_CHECK_INTERVAL => running,
            _ => {
                let running = any_process_running(&config.keep_awake_processes);
                self.process_check.set(Some((now, running)));
                running
            }
        };
        running.then_some(Hold::Program)
    }

    /// A line for the top of the menu saying what is going on, when that
    /// isn't obvious: above all, what is (or was) keeping the screen on.
    fn status(&self, config: &Config) -> Option<String> {
        if let Some(hold) = self.hold.get() {
            return Some(tf("Screen is being kept on: {}", &[hold.reason()]));
        }
        if !config.enabled {
            return None;
        }
        if let Some(schedule) = config.schedule.filter(|s| !s.contains(local_minute_of_day())) {
            return Some(tf("Not active outside {}", &[&schedule.to_string()]));
        }
        let (hold, at) = self.last_hold.get()?;
        let time = format!("{:02}:{:02}", at / 60, at % 60);
        Some(tf("Last kept on by: {} (until {})", &[hold.reason(), &time]))
    }

    /// Appends a line to the debug log, if it is switched on.
    fn log(&self, message: &str) {
        use std::io::Write;
        if !self.config.borrow().debug_log {
            return;
        }
        let path = self.config_path.with_file_name("screensleep.log");
        // Start afresh rather than let the log grow without limit.
        if std::fs::metadata(&path).is_ok_and(|file| file.len() > 1_000_000) {
            let _ = std::fs::remove_file(&path);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let mut t: SYSTEMTIME = unsafe { zeroed() };
            unsafe { GetLocalTime(&mut t) };
            let date = format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay);
            let time = format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond);
            let _ = writeln!(file, "{date} {time}  {message}");
        }
    }

    fn perform(&self, action: Action) {
        self.log(&format!("Acting: {}", action.label()));
        self.require_display(false);
        unsafe {
            match action {
                Action::ScreenOff => self.turn_screen_off(),
                Action::Lock => {
                    LockWorkStation();
                }
                Action::LockAndScreenOff => {
                    LockWorkStation();
                    self.turn_screen_off();
                }
                Action::Screensaver => {
                    SendMessageW(self.hwnd, WM_SYSCOMMAND, SC_SCREENSAVE as WPARAM, 0);
                }
                Action::Sleep => {
                    SetSuspendState(false, false, false);
                }
                Action::Hibernate => {
                    SetSuspendState(true, false, false);
                }
            }
        }
    }

    fn turn_screen_off(&self) {
        self.require_display(false);
        // Sent to our own window (DefWindowProc acts on it). Broadcasting it,
        // as is often done, can hang on any unresponsive window.
        unsafe { SendMessageW(self.hwnd, WM_SYSCOMMAND, SC_MONITORPOWER as WPARAM, SC_MONITORPOWER_OFF) };
    }

    /// "Turn screen off now": acts after a short delay, so that letting go
    /// of the mouse button or hotkey doesn't wake the screen straight away.
    fn turn_screen_off_soon(&self) {
        let delay_ms = self.config.borrow().screen_off_now_delay_seconds * 1000;
        unsafe { SetTimer(self.hwnd, TIMER_OFF_NOW, delay_ms, None) };
    }

    fn require_display(&self, required: bool) {
        if self.display_required.replace(required) != required {
            let flags = if required {
                ES_CONTINUOUS | ES_DISPLAY_REQUIRED
            } else {
                ES_CONTINUOUS
            };
            unsafe { SetThreadExecutionState(flags) };
        }
    }

    fn set_tick_interval(&self, ms: u32) {
        if self.tick_ms.replace(ms) != ms {
            unsafe { SetTimer(self.hwnd, TIMER_TICK, ms, None) };
        }
    }

    fn set_enabled(&self, enabled: bool) {
        self.update_config(|config| config.enabled = enabled);
    }

    /// Starts keeping the screen on for `minutes`, or stops if `None`.
    fn keep_on(&self, minutes: Option<u32>) {
        let until = minutes.map(|m| Instant::now() + Duration::from_secs(u64::from(m) * 60));
        self.keep_on_until.set(until);
        self.tray(NIM_MODIFY);
        self.tick();
    }

    fn keep_on_minutes_left(&self) -> Option<u32> {
        let left = self.keep_on_until.get()?.checked_duration_since(Instant::now())?;
        Some(left.as_secs().div_ceil(60) as u32)
    }

    /// What a tray click or hotkey does. `announce` gives feedback for
    /// actions that otherwise have no visible effect.
    fn click(&self, action: ClickAction, announce: bool) {
        match action {
            ClickAction::Menu => self.show_menu(),
            ClickAction::Toggle => {
                let enabled = !self.config.borrow().enabled;
                self.set_enabled(enabled);
                if announce {
                    self.notify(if enabled {
                        t("screenSleep is on.")
                    } else {
                        t("screenSleep is off.")
                    });
                }
            }
            ClickAction::ScreenOff => self.turn_screen_off_soon(),
            ClickAction::KeepOn => {
                let minutes = self.config.borrow().keep_on_minutes;
                let start = self.keep_on_minutes_left().is_none();
                self.keep_on(start.then_some(minutes));
                if announce && start {
                    self.notify(&tf("Keeping the screen on for {}.", &[&minutes_label(minutes)]));
                } else if announce {
                    self.notify(t("No longer keeping the screen on."));
                }
            }
            ClickAction::Nothing => {}
        }
    }

    /// Shows the countdown for a few seconds, to see how it looks.
    pub(crate) fn preview(&self) {
        self.preview_until.set(Some(Instant::now() + PREVIEW_DURATION));
        self.tick();
    }

    // ----- Countdown window -----

    /// Shows the countdown with `seconds` on it, or hides it for `None`.
    fn set_overlay(&self, seconds: Option<u32>, config: &Config) {
        let before = self.overlay_seconds.replace(seconds);
        if let Some(number) = seconds {
            self.overlay_number.set(number);
        }
        match (before, seconds) {
            (None, Some(_)) => {
                self.place_overlays(config);
                self.fade_to(1.0, config);
            }
            (Some(_), None) => self.fade_to(0.0, config),
            (Some(_), Some(_)) if before != seconds => self.each_overlay(|window| unsafe {
                InvalidateRect(window, null(), 0);
            }),
            _ => {}
        }

        // Animate the outline only while there is something to animate.
        let pulse = seconds.is_some() && config.warning_outline && config.warning_outline_pulse_ms > 0;
        if self.pulsing.replace(pulse) != pulse {
            unsafe {
                if pulse {
                    SetTimer(self.hwnd, TIMER_PULSE, PULSE_FRAME_MS, None);
                } else {
                    KillTimer(self.hwnd, TIMER_PULSE);
                }
            }
        }
    }

    /// Starts fading the countdown (and the dimming behind it) in or out,
    /// or switches straight to the end state if fading is turned off.
    fn fade_to(&self, target: f32, config: &Config) {
        self.fade_target.set(target);
        if config.warning_fade {
            if !self.fading.replace(true) {
                unsafe { SetTimer(self.hwnd, TIMER_FADE, FADE_FRAME_MS, None) };
            }
        } else {
            self.fade_level.set(target);
            self.apply_fade(config);
        }
    }

    /// One frame of the fade.
    fn fade_step(&self) {
        let (level, target) = (self.fade_level.get(), self.fade_target.get());
        let step = FADE_FRAME_MS as f32 / FADE_MS;
        let level = if level < target {
            (level + step).min(target)
        } else {
            (level - step).max(target)
        };
        self.fade_level.set(level);
        self.apply_fade(&self.config());
        if level == target {
            self.fading.set(false);
            unsafe { KillTimer(self.hwnd, TIMER_FADE) };
        }
    }

    /// Sets how see-through the countdown and dimming windows are, from the
    /// current fade level, and hides them once fully faded out.
    fn apply_fade(&self, config: &Config) {
        let level = self.fade_level.get();
        let hide = level <= 0.0 && self.fade_target.get() <= 0.0;
        let alpha = |percent: u32| (level * percent as f32 * 2.55).round().clamp(0.0, 255.0) as u8;
        let apply = |windows: Vec<HWND>, percent: u32| {
            for window in windows {
                unsafe {
                    SetLayeredWindowAttributes(window, 0, alpha(percent), LWA_ALPHA);
                    if hide {
                        ShowWindow(window, SW_HIDE);
                    }
                }
            }
        };
        // Copied out first: these calls can run window procedures, which
        // must not find the lists borrowed.
        let (overlays, dims) = (self.overlays.borrow().clone(), self.dims.borrow().clone());
        apply(overlays, config.warning_opacity_percent);
        apply(dims, config.warning_dim_percent);
    }

    /// One frame of the pulse: repaints just the outline of each countdown
    /// window, leaving the text alone so nothing flickers.
    fn pulse_outlines(&self) {
        let config = self.config();
        self.each_overlay(|window| unsafe {
            let hdc = GetDC(window);
            let mut rect: RECT = zeroed();
            GetClientRect(window, &mut rect);
            self.draw_outline(hdc, rect, &config);
            ReleaseDC(window, hdc);
        });
    }

    /// Draws the outline in its current colour and returns the area inside
    /// it (all of `rect` if the outline is switched off).
    fn draw_outline(&self, hdc: HDC, rect: RECT, config: &Config) -> RECT {
        if !config.warning_outline {
            return rect;
        }
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        let level = policy::pulse_level(elapsed_ms, u64::from(config.warning_outline_pulse_ms));
        let color = fade(config.warning_outline_color, config.warning_outline_pulse_color, level);

        let t = (rect.bottom * config.warning_outline_width_percent as i32 / 100).max(1);
        let inner = RECT {
            left: rect.left + t,
            top: rect.top + t,
            right: rect.right - t,
            bottom: rect.bottom - t,
        };
        let edges = [
            RECT {
                bottom: inner.top,
                ..rect
            },
            RECT {
                top: inner.bottom,
                ..rect
            },
            RECT {
                right: inner.left,
                ..rect
            },
            RECT {
                left: inner.right,
                ..rect
            },
        ];
        unsafe {
            let brush = CreateSolidBrush(colorref(color));
            for edge in &edges {
                FillRect(hdc, edge, brush);
            }
            DeleteObject(brush);
        }
        inner
    }

    fn each_overlay(&self, f: impl Fn(HWND)) {
        // Copied out first: these calls can run window procedures, which
        // must not find the list borrowed.
        let windows = self.overlays.borrow().clone();
        windows.into_iter().for_each(f);
    }

    /// Makes sure `pool` holds at least `count` windows of the given class,
    /// creating more as needed, and returns them all.
    fn window_pool(&self, pool: &RefCell<Vec<HWND>>, class: PCWSTR, style: WINDOW_EX_STYLE, count: usize) -> Vec<HWND> {
        let mut windows = pool.borrow().clone();
        while windows.len() < count {
            // These never take focus, so they can't swallow keystrokes
            // meant for whatever the user was doing.
            let style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | style;
            let window = unsafe {
                CreateWindowExW(
                    style,
                    class,
                    w!("screenSleep"),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    null_mut(),
                    null_mut(),
                    self.hinstance,
                    null(),
                )
            };
            if window.is_null() {
                break;
            }
            windows.push(window);
        }
        pool.replace(windows.clone());
        windows
    }

    /// Shows a countdown window on each monitor the settings call for, with
    /// the rest of that monitor dimmed behind it if wanted.
    fn place_overlays(&self, config: &Config) {
        let monitors = unsafe {
            let mut cursor = POINT { x: 0, y: 0 };
            GetCursorPos(&mut cursor);
            match config.warning_monitor {
                WarningMonitor::All => all_monitors(),
                WarningMonitor::Cursor => vec![MonitorFromPoint(cursor, MONITOR_DEFAULTTOPRIMARY)],
                WarningMonitor::Primary => vec![MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)],
            }
        };
        let level = self.fade_level.get();
        let alpha = |percent: u32| (level * percent as f32 * 2.55).round().clamp(0.0, 255.0) as u8;

        // The dimming goes down first so that the countdown sits on top of
        // it. Mouse clicks pass straight through it.
        let dim_count = if config.warning_dim_percent > 0 {
            monitors.len()
        } else {
            0
        };
        let dims = self.window_pool(&self.dims, w!("screenSleep.Dim"), WS_EX_TRANSPARENT, dim_count);
        for (index, window) in dims.into_iter().enumerate() {
            unsafe {
                let Some(&monitor) = monitors.get(index).filter(|_| index < dim_count) else {
                    ShowWindow(window, SW_HIDE);
                    continue;
                };
                let mut info: MONITORINFO = zeroed();
                info.cbSize = size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(monitor, &mut info);
                let screen = info.rcMonitor;
                SetLayeredWindowAttributes(window, 0, alpha(config.warning_dim_percent), LWA_ALPHA);
                SetWindowPos(
                    window,
                    HWND_TOPMOST,
                    screen.left,
                    screen.top,
                    screen.right - screen.left,
                    screen.bottom - screen.top,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }

        let overlays = self.window_pool(&self.overlays, w!("screenSleep.Overlay"), 0, monitors.len());
        for (index, window) in overlays.into_iter().enumerate() {
            match monitors.get(index) {
                Some(&monitor) => self.place_overlay(window, monitor, alpha(config.warning_opacity_percent), config),
                // Left over from when more monitors were in use.
                None => unsafe {
                    ShowWindow(window, SW_HIDE);
                },
            }
        }
    }

    /// Sizes, positions and shows one countdown window on `monitor`.
    fn place_overlay(&self, window: HWND, monitor: HMONITOR, alpha: u8, config: &Config) {
        unsafe {
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(monitor, &mut info);

            // Sized relative to the monitor, so it looks the same at any
            // resolution or scaling. Kept clear of the taskbar.
            let area = info.rcWork;
            let (area_w, area_h) = ((area.right - area.left).max(1), (area.bottom - area.top).max(1));
            let screen_h = info.rcMonitor.bottom - info.rcMonitor.top;
            let height = (screen_h / 4 * config.warning_size_percent as i32 / 100).clamp(60, area_h);
            let width = (height * 5 / 2).min(area_w);
            let margin = screen_h / 24;

            let (left, center, right) = (
                area.left + margin,
                area.left + (area_w - width) / 2,
                area.right - margin - width,
            );
            let (top, middle, bottom) = (
                area.top + margin,
                area.top + (area_h - height) / 2,
                area.bottom - margin - height,
            );
            let (x, y) = match config.warning_position {
                Position::Center => (center, middle),
                Position::Top => (center, top),
                Position::Bottom => (center, bottom),
                Position::TopLeft => (left, top),
                Position::TopRight => (right, top),
                Position::BottomLeft => (left, bottom),
                Position::BottomRight => (right, bottom),
            };

            // Rounded corners are drawn by Windows 11; on Windows 10 these
            // calls fail harmlessly and the corners stay square.
            let corners = if config.warning_rounded_corners {
                DWMWCP_ROUND
            } else {
                DWMWCP_DONOTROUND
            };
            let attribute_size = size_of::<u32>() as u32;
            DwmSetWindowAttribute(
                window,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&raw const corners).cast(),
                attribute_size,
            );
            let no_border = DWMWA_COLOR_NONE;
            DwmSetWindowAttribute(
                window,
                DWMWA_BORDER_COLOR as u32,
                (&raw const no_border).cast(),
                attribute_size,
            );

            SetLayeredWindowAttributes(window, 0, alpha, LWA_ALPHA);
            SetWindowPos(
                window,
                HWND_TOPMOST,
                x.max(area.left),
                y.max(area.top),
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            InvalidateRect(window, null(), 0);
        }
    }

    fn paint_overlay(&self, window: HWND) {
        let config = self.config();
        let seconds = self.overlay_number.get();
        let font_name = wide(&config.warning_font);
        unsafe {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(window, &mut ps);
            let mut rect: RECT = zeroed();
            GetClientRect(window, &mut rect);
            let height = rect.bottom;

            let background = CreateSolidBrush(colorref(config.warning_background));
            let inside = self.draw_outline(hdc, rect, &config);
            FillRect(hdc, &inside, background);
            DeleteObject(background);
            SetBkMode(hdc, TRANSPARENT as _);

            let line = |text: &str, top: i32, bottom: i32, weight: i32, color: COLORREF| {
                let font = CreateFontW(
                    -(bottom - top) * 3 / 4,
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as _,
                    0,
                    0,
                    CLEARTYPE_QUALITY as _,
                    0,
                    font_name.as_ptr(),
                );
                let previous = SelectObject(hdc, font);
                SetTextColor(hdc, color);
                let text: Vec<u16> = text.encode_utf16().collect();
                let mut area = RECT {
                    left: rect.left,
                    top,
                    right: rect.right,
                    bottom,
                };
                let format = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS;
                DrawTextW(hdc, text.as_ptr(), text.len() as i32, &mut area, format);
                SelectObject(hdc, previous);
                DeleteObject(font);
            };

            let title = if config.warning_title.is_empty() {
                t(config.action.countdown_title())
            } else {
                &config.warning_title
            };
            // The default hint is translated; one the user wrote is not.
            let hint = t(&config.warning_hint);
            let show_hint = config.warning_show_hint && !config.warning_hint.is_empty();
            // Without the hint, the number takes up the room it leaves.
            let (title_end, number_end) = (height * 30 / 100, height * if show_hint { 78 } else { 94 } / 100);
            let text_color = colorref(config.warning_text_color);
            line(title, height * 8 / 100, title_end, FW_NORMAL as i32, text_color);
            line(
                &seconds.to_string(),
                title_end,
                number_end,
                FW_SEMIBOLD as i32,
                colorref(config.warning_number_color),
            );
            if show_hint {
                let faded = colorref(blend(config.warning_text_color, config.warning_background));
                line(hint, number_end, height * 92 / 100, FW_NORMAL as i32, faded);
            }

            EndPaint(window, &ps);
        }
    }

    // ----- Tray menu -----

    fn show_menu(&self) {
        let config = self.config();
        unsafe {
            let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
            let add = |menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str| {
                AppendMenuW(menu, flags, id, wide(text).as_ptr());
            };
            let separator = |menu: HMENU| {
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
            };
            // Adds a ticked list of numeric choices, including the current
            // value if it isn't one of the usual ones.
            let choices = |menu: HMENU, usual: &[u32], current: u32, base: usize, label: fn(u32) -> String| {
                let mut values = usual.to_vec();
                if !values.contains(&current) {
                    values.push(current);
                    values.sort_unstable();
                }
                for value in values {
                    add(
                        menu,
                        MF_STRING | check(value == current),
                        base + value as usize,
                        &label(value),
                    );
                }
            };

            let timeouts = CreatePopupMenu();
            choices(
                timeouts,
                &config.timeout_menu,
                config.timeout_minutes,
                CMD_TIMEOUT_BASE,
                minutes_label,
            );

            let actions = CreatePopupMenu();
            for (index, action) in Action::ALL.iter().enumerate() {
                let flags = MF_STRING | check(*action == config.action);
                add(actions, flags, CMD_ACTION_BASE + index, t(action.label()));
            }

            let positions = CreatePopupMenu();
            for (index, position) in Position::ALL.iter().enumerate() {
                let flags = MF_STRING | check(*position == config.warning_position);
                add(positions, flags, CMD_POSITION_BASE + index, t(position.label()));
            }

            let monitors = CreatePopupMenu();
            for (index, monitor) in WarningMonitor::ALL.iter().enumerate() {
                let flags = MF_STRING | check(*monitor == config.warning_monitor);
                add(monitors, flags, CMD_MONITOR_BASE + index, t(monitor.label()));
            }

            let countdown = CreatePopupMenu();
            add(
                countdown,
                MF_STRING | check(config.warning_seconds == 0),
                CMD_WARNING_BASE,
                t("None"),
            );
            let current = if config.warning_seconds == 0 {
                WARNING_CHOICES[0]
            } else {
                config.warning_seconds
            };
            choices(countdown, &WARNING_CHOICES, current, CMD_WARNING_BASE, seconds_label);
            if config.warning_seconds == 0 {
                CheckMenuItem(countdown, (CMD_WARNING_BASE + current as usize) as u32, MF_UNCHECKED);
            }
            separator(countdown);
            add(countdown, MF_POPUP, monitors as usize, t("Show on"));
            add(countdown, MF_POPUP, positions as usize, t("Position"));
            add(
                countdown,
                MF_STRING | check(config.warning_sound),
                CMD_WARNING_SOUND,
                t("Play a sound"),
            );
            add(countdown, MF_STRING, CMD_PREVIEW, t("Preview"));

            let keep_on = CreatePopupMenu();
            for minutes in KEEP_ON_CHOICES {
                add(
                    keep_on,
                    MF_STRING,
                    CMD_KEEP_ON_BASE + minutes as usize,
                    &minutes_label(minutes),
                );
            }

            let keep_awake = CreatePopupMenu();
            let flags = |on: bool| MF_STRING | check(on);
            add(
                keep_awake,
                flags(config.media_keeps_awake),
                CMD_MEDIA,
                t("Music or video is playing"),
            );
            add(
                keep_awake,
                flags(config.fullscreen_keeps_awake),
                CMD_FULLSCREEN,
                t("A full-screen app is in front"),
            );
            add(
                keep_awake,
                flags(config.edge_keeps_awake),
                CMD_EDGE,
                t("The cursor is at a chosen screen edge"),
            );

            let icons = CreatePopupMenu();
            for (index, style) in IconStyle::ALL.iter().enumerate() {
                add(
                    icons,
                    flags(*style == config.tray_icon),
                    CMD_ICON_BASE + index,
                    t(style.label()),
                );
            }

            let menu = CreatePopupMenu();
            if let Some(status) = self.status(&config) {
                add(menu, MF_STRING | MF_GRAYED, 0, &status);
                separator(menu);
            }
            add(
                menu,
                flags(config.enabled),
                CMD_ENABLED,
                t(config.action.when_idle_label()),
            );
            let after = tf("After {}", &[&minutes_label(config.timeout_minutes)]);
            add(menu, MF_POPUP, timeouts as usize, &after);
            add(menu, MF_POPUP, actions as usize, t("When idle"));
            add(menu, MF_POPUP, countdown as usize, t("Countdown"));
            add(menu, MF_POPUP, keep_awake as usize, t("Don't count as idle while"));
            separator(menu);
            add(menu, MF_STRING, CMD_OFF_NOW, t("Turn screen off now"));
            add(menu, MF_POPUP, keep_on as usize, t("Keep screen on for"));
            if let Some(left) = self.keep_on_minutes_left() {
                let text = tf("Stop keeping screen on ({} left)", &[&minutes_label(left)]);
                add(menu, MF_STRING | MF_CHECKED, CMD_KEEP_ON_STOP, &text);
            }
            separator(menu);
            add(menu, MF_POPUP, icons as usize, t("Tray icon"));
            add(menu, flags(autostart_enabled()), CMD_AUTOSTART, t("Start with Windows"));
            add(menu, MF_STRING, CMD_SETTINGS, t("Settings..."));
            separator(menu);
            add(menu, MF_STRING, CMD_EXIT, t("Exit"));

            let mut cursor: POINT = zeroed();
            GetCursorPos(&mut cursor);
            // Required so the menu closes when the user clicks elsewhere.
            SetForegroundWindow(self.hwnd);
            let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON;
            let command = TrackPopupMenu(menu, flags, cursor.x, cursor.y, 0, self.hwnd, null());
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);

            self.run_command(command as usize);
        }
    }

    fn run_command(&self, command: usize) {
        let pick = |base: usize| command - base;
        match command {
            CMD_ENABLED => self.update_config(|c| c.enabled = !c.enabled),
            CMD_MEDIA => self.update_config(|c| c.media_keeps_awake = !c.media_keeps_awake),
            CMD_FULLSCREEN => self.update_config(|c| c.fullscreen_keeps_awake = !c.fullscreen_keeps_awake),
            CMD_EDGE => self.update_config(|c| c.edge_keeps_awake = !c.edge_keeps_awake),
            CMD_WARNING_SOUND => self.update_config(|c| c.warning_sound = !c.warning_sound),
            CMD_OFF_NOW => self.turn_screen_off_soon(),
            CMD_KEEP_ON_STOP => self.keep_on(None),
            CMD_PREVIEW => self.preview(),
            CMD_AUTOSTART => set_autostart(!autostart_enabled()),
            CMD_SETTINGS => settings::open(self.hinstance),
            CMD_EXIT => unsafe {
                DestroyWindow(self.hwnd);
            },
            _ if command >= CMD_TIMEOUT_BASE => {
                self.update_config(|c| c.timeout_minutes = pick(CMD_TIMEOUT_BASE) as u32)
            }
            _ if command >= CMD_KEEP_ON_BASE => self.keep_on(Some(pick(CMD_KEEP_ON_BASE) as u32)),
            _ if command >= CMD_WARNING_BASE => {
                self.update_config(|c| c.warning_seconds = pick(CMD_WARNING_BASE) as u32)
            }
            _ if command >= CMD_MONITOR_BASE => {
                if let Some(&monitor) = WarningMonitor::ALL.get(pick(CMD_MONITOR_BASE)) {
                    self.update_config(|c| c.warning_monitor = monitor);
                }
            }
            _ if command >= CMD_POSITION_BASE => {
                if let Some(&position) = Position::ALL.get(pick(CMD_POSITION_BASE)) {
                    self.update_config(|c| c.warning_position = position);
                }
            }
            _ if command >= CMD_ICON_BASE => {
                if let Some(&style) = IconStyle::ALL.get(pick(CMD_ICON_BASE)) {
                    self.update_config(|c| c.tray_icon = style);
                }
            }
            _ if command >= CMD_ACTION_BASE => {
                if let Some(&action) = Action::ALL.get(pick(CMD_ACTION_BASE)) {
                    self.update_config(|c| c.action = action);
                }
            }
            _ => {}
        }
    }

    // ----- Settings -----

    /// Applies a change made through the menu or a hotkey, and saves it.
    fn update_config(&self, change: impl FnOnce(&mut Config)) {
        let mut config = self.config();
        change(&mut config);
        if config != *self.config.borrow() {
            self.set_config(config);
            self.save_config();
            self.tick();
        }
    }

    /// Applies everything from the settings window.
    pub(crate) fn apply_settings(&self, config: Config, autostart: bool) {
        if config != *self.config.borrow() {
            self.set_config(config);
            self.save_config();
            self.tick();
        }
        if autostart != autostart_enabled() {
            set_autostart(autostart);
        }
    }

    /// Opens the settings file in Notepad.
    pub(crate) fn open_settings_file(&self) {
        let path = wide(&format!("\"{}\"", self.config_path.display()));
        unsafe {
            ShellExecuteW(
                null_mut(),
                w!("open"),
                w!("notepad.exe"),
                path.as_ptr(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }

    /// Puts new settings into effect.
    fn set_config(&self, config: Config) {
        lang::set_language(lang::resolve(config.language, unsafe { GetUserDefaultUILanguage() }));
        let old = self.config.replace(config.clone());
        let hotkeys = |c: &Config| {
            [
                c.hotkey_toggle.clone(),
                c.hotkey_screen_off.clone(),
                c.hotkey_keep_on.clone(),
            ]
        };
        if hotkeys(&old) != hotkeys(&config) {
            self.register_hotkeys();
        }
        self.tray(NIM_MODIFY);
        // If the countdown is up (most likely a preview), restyle it live.
        if self.overlay_seconds.get().is_some() {
            self.place_overlays(&config);
        }
    }

    fn save_config(&self) {
        // If the folder isn't writable the app still works; the settings just
        // won't survive a restart.
        let _ = self.config.borrow().save(&self.config_path);
        self.config_mtime.set(self.config_file_mtime());
    }

    fn config_file_mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.config_path).and_then(|m| m.modified()).ok()
    }

    /// Picks up edits made to the settings file by hand.
    fn reload_config_if_changed(&self) {
        let mtime = self.config_file_mtime();
        if mtime.is_none() || mtime == self.config_mtime.get() {
            return;
        }
        self.config_mtime.set(mtime);
        if let Some((config, problems)) = Config::load(&self.config_path) {
            if config != *self.config.borrow() {
                self.set_config(config);
                self.log("Settings file changed; reloaded");
            }
            if !problems.is_empty() {
                self.report_problems(&problems);
            }
        }
    }

    fn report_problems(&self, problems: &[String]) {
        self.log(&format!("Settings problems: {}", problems.join(" ")));
        self.balloon(
            NIIF_WARNING,
            &tf("Problem in the settings file. {}", &[&problems.join(" ")]),
        );
    }

    fn register_hotkeys(&self) {
        let config = self.config();
        let hotkeys = [
            (HOTKEY_TOGGLE, &config.hotkey_toggle),
            (HOTKEY_SCREEN_OFF, &config.hotkey_screen_off),
            (HOTKEY_KEEP_ON, &config.hotkey_keep_on),
        ];
        for (id, text) in hotkeys {
            unsafe {
                UnregisterHotKey(self.hwnd, id);
                let Ok(hotkey) = hotkey::parse(text) else {
                    continue; // Empty: no hotkey wanted.
                };
                if RegisterHotKey(self.hwnd, id, hotkey.modifiers | MOD_NOREPEAT, hotkey.key) == 0 {
                    let message = tf(
                        "Couldn't set up the hotkey {}. Another program may be using it.",
                        &[text],
                    );
                    self.balloon(NIIF_WARNING, &message);
                }
            }
        }
    }

    // ----- Tray icon -----

    fn tray_data(&self) -> NOTIFYICONDATAW {
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = TRAY_ID;
        data
    }

    /// Adds (`NIM_ADD`) or refreshes (`NIM_MODIFY`) the tray icon.
    fn tray(&self, action: NOTIFY_ICON_MESSAGE) {
        let config = self.config();
        let status = if self.keep_on_minutes_left().is_some() {
            t("Keeping the screen on").to_string()
        } else if config.enabled {
            let after = minutes_label(config.timeout_minutes);
            tf("{} (after {})", &[t(config.action.when_idle_label()), &after])
        } else {
            t("Switched off").to_string()
        };
        let light = match config.tray_icon {
            IconStyle::Auto => taskbar_is_dark(),
            IconStyle::Light => true,
            IconStyle::Dark => false,
        };
        let mut data = self.tray_data();
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = if light { self.icon_light } else { self.icon_dark };
        copy_wide(&mut data.szTip, &format!("{APP_NAME}: {status}"));
        unsafe { Shell_NotifyIconW(action, &data) };
    }

    /// An informational notification, unless they are switched off.
    fn notify(&self, text: &str) {
        if self.config.borrow().notifications {
            self.balloon(NIIF_INFO, text);
        }
    }

    fn balloon(&self, kind: NOTIFY_ICON_INFOTIP_FLAGS, text: &str) {
        let mut data = self.tray_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = kind;
        copy_wide(&mut data.szInfoTitle, APP_NAME);
        copy_wide(&mut data.szInfo, text);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam == TIMER_TICK => {
            with_app(App::tick);
            0
        }
        WM_TIMER if wparam == TIMER_FADE => {
            with_app(App::fade_step);
            0
        }
        WM_TIMER if wparam == TIMER_PULSE => {
            with_app(App::pulse_outlines);
            0
        }
        WM_TIMER if wparam == TIMER_OFF_NOW => {
            KillTimer(hwnd, TIMER_OFF_NOW);
            with_app(App::turn_screen_off);
            0
        }
        WM_TRAY => {
            with_app(|app| match lparam as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => app.show_menu(),
                WM_LBUTTONUP => app.click(app.config().tray_left_click, true),
                WM_MBUTTONUP => app.click(app.config().tray_middle_click, true),
                _ => {}
            });
            0
        }
        WM_HOTKEY => {
            with_app(|app| match wparam as i32 {
                HOTKEY_TOGGLE => app.click(ClickAction::Toggle, true),
                HOTKEY_SCREEN_OFF => app.click(ClickAction::ScreenOff, true),
                HOTKEY_KEEP_ON => app.click(ClickAction::KeepOn, true),
                _ => {}
            });
            0
        }
        // The taskbar may have switched between light and dark.
        WM_SETTINGCHANGE => {
            with_app(|app| app.tray(NIM_MODIFY));
            0
        }
        WM_DESTROY => {
            with_app(|app| Shell_NotifyIconW(NIM_DELETE, &app.tray_data()));
            PostQuitMessage(0);
            0
        }
        // Explorer restarted: the tray icon is gone and must be added again.
        _ if with_app(|app| msg == app.taskbar_created) == Some(true) => {
            with_app(|app| app.tray(NIM_ADD));
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT if with_app(|app| app.paint_overlay(hwnd)).is_some() => 0,
        WM_ERASEBKGND => 1,
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Every monitor that is part of the desktop.
fn all_monitors() -> Vec<HMONITOR> {
    unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> BOOL {
        (*(list as *mut Vec<HMONITOR>)).push(monitor);
        1
    }
    let mut monitors: Vec<HMONITOR> = Vec::new();
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(collect), &raw mut monitors as LPARAM) };
    monitors
}

/// Milliseconds since the last keyboard or mouse input, system-wide.
fn input_idle_ms() -> u64 {
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info) == 0 {
            return 0;
        }
        // Both values are 32-bit tick counts that wrap every 49.7 days;
        // wrapping subtraction gives the right answer across the wrap.
        u64::from(GetTickCount().wrapping_sub(info.dwTime))
    }
}

/// True while the cursor rests on one of the chosen edges of the desktop.
fn cursor_on_edge(config: &Config) -> bool {
    unsafe {
        let mut cursor: POINT = zeroed();
        if GetCursorPos(&mut cursor) == 0 {
            return false;
        }
        let (left, top) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let right = left + GetSystemMetrics(SM_CXVIRTUALSCREEN) - 1;
        let bottom = top + GetSystemMetrics(SM_CYVIRTUALSCREEN) - 1;
        let edges = config.keep_awake_edges;
        (edges.left && cursor.x <= left)
            || (edges.top && cursor.y <= top)
            || (edges.right && cursor.x >= right)
            || (edges.bottom && cursor.y >= bottom)
    }
}

/// True while a button, trigger or stick on any connected game controller
/// is being pressed. Sticks and triggers must pass a dead zone, so a worn
/// controller resting on the desk doesn't count.
fn controller_in_use() -> bool {
    const STICK_DEAD_ZONE: i32 = 9000;
    const TRIGGER_DEAD_ZONE: u8 = 40;
    (0..4).any(|index| unsafe {
        let mut state: XINPUT_STATE = zeroed();
        if XInputGetState(index, &mut state) != ERROR_SUCCESS {
            return false; // No controller in this slot.
        }
        let pad = state.Gamepad;
        let sticks = [pad.sThumbLX, pad.sThumbLY, pad.sThumbRX, pad.sThumbRY];
        pad.wButtons != 0
            || pad.bLeftTrigger > TRIGGER_DEAD_ZONE
            || pad.bRightTrigger > TRIGGER_DEAD_ZONE
            || sticks.iter().any(|&axis| i32::from(axis).abs() > STICK_DEAD_ZONE)
    })
}

/// True while a full-screen app (a game, a presentation) is in front.
fn fullscreen_app_in_front() -> bool {
    unsafe {
        let mut state = 0;
        SHQueryUserNotificationState(&mut state) == 0
            && matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
    }
}

/// True if any program with one of these (lower-case) names is running.
fn any_process_running(names: &[String]) -> bool {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut more = Process32FirstW(snapshot, &mut entry);
        while more != 0 && !found {
            let length = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..length]).to_lowercase();
            found = config::name_matches(names, &exe);
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        found
    }
}

fn on_battery() -> bool {
    unsafe {
        let mut status: SYSTEM_POWER_STATUS = zeroed();
        GetSystemPowerStatus(&mut status) != 0 && status.ACLineStatus == 0
    }
}

/// Minutes since midnight, local time.
fn local_minute_of_day() -> u32 {
    unsafe {
        let mut time: SYSTEMTIME = zeroed();
        GetLocalTime(&mut time);
        u32::from(time.wHour) * 60 + u32::from(time.wMinute)
    }
}

/// Whether the taskbar uses the dark theme. Versions of Windows without the
/// setting have a dark taskbar.
fn taskbar_is_dark() -> bool {
    unsafe {
        let mut light: u32 = 0;
        let mut bytes = size_of::<u32>() as u32;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            THEME_KEY,
            THEME_VALUE,
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut light).cast(),
            &mut bytes,
        );
        status != ERROR_SUCCESS || light == 0
    }
}

/// The command currently registered to run at sign-in, if any.
fn autostart_command() -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 1024];
        let mut bytes = (buffer.len() * 2) as u32;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        );
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..len]))
    }
}

pub(crate) fn autostart_enabled() -> bool {
    autostart_command().is_some()
}

fn expected_autostart_command() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    format!("\"{}\"", exe.display())
}

fn set_autostart(on: bool) {
    unsafe {
        if on {
            let command = wide(&expected_autostart_command());
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            );
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
        }
    }
}

/// A null-terminated UTF-16 copy of `text`.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies `text` into a fixed-size UTF-16 buffer, truncating if needed and
/// always leaving it null-terminated.
fn copy_wide(buffer: &mut [u16], text: &str) {
    let max = buffer.len() - 1;
    for (slot, unit) in buffer.iter_mut().zip(text.encode_utf16().take(max)) {
        *slot = unit;
    }
}

fn colorref(color: Rgb) -> COLORREF {
    u32::from(color.0) | u32::from(color.1) << 8 | u32::from(color.2) << 16
}

/// The colour `amount` (0.0 to 1.0) of the way from `from` to `to`.
fn fade(from: Rgb, to: Rgb, amount: f32) -> Rgb {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Rgb(mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

/// A colour between `a` and `b`, a little closer to `a`.
fn blend(a: Rgb, b: Rgb) -> Rgb {
    let mix = |x: u8, y: u8| ((u32::from(x) * 6 + u32::from(y) * 4) / 10) as u8;
    Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}
