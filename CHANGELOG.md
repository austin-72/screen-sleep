# Changelog

## 2.1.0

### New

- **Settings window.** "Settings..." in the tray menu opens a window with
  every setting, grouped into tabs, with a colour picker for the colours and
  an "Apply and preview" button. The settings file is still there and still
  reloads as you save it.
- **Translations:** Spanish, German and French, chosen automatically from the
  Windows language or set by hand.
- **Countdown polish:** it fades in and out, has rounded corners on
  Windows 11, and darkens the rest of the screen behind it. Each can be
  switched off.

### Changed

- "All settings..." in the menu is now "Settings..." and opens the window
  rather than the file.
- The countdown darkens the rest of the screen by 30% by default; set
  `warning_dim_percent = 0` for the old look.

## 2.0.0

A complete rewrite of the original C#/WinForms app in Rust. One small `.exe`,
no .NET runtime, and it lives entirely in the system tray.

### New

- **Choice of action** at timeout: turn the screen off, lock, lock and turn
  the screen off, start the screen saver, sleep or hibernate.
- **Media keeps the screen on.** Sustained sound counts as activity; short
  sounds such as notification chimes don't. Sound from chosen programs (a
  voice call, say) can be ignored.
- **Game controllers count as input**, which Windows itself doesn't do.
- **More ways to hold the screen on:** a full-screen app in front, a list of
  programs, the cursor parked at a screen edge, or "Keep screen on for ..."
  from the menu.
- **"Why is my screen still on?"** The menu says what is keeping the screen
  on, or what last did and until when.
- **Countdown** on every monitor, with configurable length, position, size,
  opacity, colours, font, text, sound, and a pulsing outline. A preview shows
  changes live.
- **Separate timeout on battery**, and an optional daily **schedule**.
- **Global hotkeys** to switch on/off, turn the screen off now, and keep the
  screen on.
- **Tray icon** that matches a light or dark taskbar; configurable left and
  middle click.
- **Settings file** with a comment on every setting, reloaded the moment it is
  saved, with a notification if a line can't be understood.
- An optional **debug log**.
- A new icon.

### Changed from 1.x

- There is no settings window; everything is in the tray menu and the
  settings file.
- The app no longer copies itself into `%APPDATA%`. It runs from wherever it
  is, and "Start with Windows" points at that location.
- Windows' own display timeout is only held off while screenSleep is switched
  on, rather than whenever it is running.
- Holding the screen on by parking the cursor at the top or left edge is now
  off by default.
- A fresh install starts switched on.
- Settings from 1.x are imported automatically.

### Fixed

- The timeout could be missed entirely if a timer tick arrived late.
- The countdown took keyboard focus away from the app in use.
- Unticking "Start with computer" could crash if the registry entry was
  missing.
- Turning the screen off could hang if another program was not responding.
- More than one copy could run at once.
