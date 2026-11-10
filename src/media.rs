//! Detects whether anything is audible, by reading the peak meters of the
//! sound output devices. This catches music and video from any app without
//! needing each player's cooperation; silent video is not detected. Telling
//! sustained media from brief chimes is `policy::SoundTracker`'s job.

use windows::core::{Interface, Result, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::config::name_matches;

/// Peak level (0.0 to 1.0) above which output counts as sound rather than
/// digital silence.
const AUDIBLE_PEAK: f32 = 0.0005;

pub struct AudioMonitor {
    devices: IMMDeviceEnumerator,
}

impl AudioMonitor {
    /// Must be called on the thread that will use the monitor.
    pub fn new() -> Option<AudioMonitor> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().ok()?;
            let devices = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            Some(AudioMonitor { devices })
        }
    }

    /// True if any active output device (speakers, headphones, ...) is
    /// producing sound right now, other than from the programs in `ignore`
    /// (lower-case executable names). Devices are looked up each time, so
    /// plugging in headphones or switching output needs no special handling.
    pub fn is_audible(&self, ignore: &[String]) -> bool {
        unsafe {
            let Ok(outputs) = self.devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else {
                return false;
            };
            (0..outputs.GetCount().unwrap_or(0)).any(|index| {
                outputs
                    .Item(index)
                    .and_then(|device| {
                        if ignore.is_empty() {
                            device_is_audible(&device)
                        } else {
                            wanted_program_is_audible(&device, ignore)
                        }
                    })
                    .unwrap_or(false)
            })
        }
    }
}

/// Whether the device as a whole is producing sound.
unsafe fn device_is_audible(device: &IMMDevice) -> Result<bool> {
    let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None)?;
    Ok(meter.GetPeakValue()? > AUDIBLE_PEAK)
}

/// Whether any program not in `ignore` is producing sound on the device.
/// Slower than `device_is_audible`: it looks at each program's audio
/// separately.
unsafe fn wanted_program_is_audible(device: &IMMDevice, ignore: &[String]) -> Result<bool> {
    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
    let sessions = manager.GetSessionEnumerator()?;
    for index in 0..sessions.GetCount()? {
        let session = sessions.GetSession(index)?;
        let audible = session
            .cast::<IAudioMeterInformation>()
            .and_then(|meter| meter.GetPeakValue())
            .is_ok_and(|peak| peak > AUDIBLE_PEAK);
        if !audible {
            continue;
        }
        let program = session
            .cast::<IAudioSessionControl2>()
            .and_then(|session| session.GetProcessId())
            .ok()
            .and_then(|id| program_name(id));
        // Sound we can't attribute to a program (system sounds) counts.
        if !program.is_some_and(|name| name_matches(ignore, &name)) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The lower-case executable name (`discord.exe`) of a running process.
unsafe fn program_name(process_id: u32) -> Option<String> {
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length);
    let _ = CloseHandle(process);
    result.ok()?;
    let path = String::from_utf16_lossy(&buffer[..length as usize]).to_lowercase();
    path.rsplit('\\').next().map(str::to_string)
}
