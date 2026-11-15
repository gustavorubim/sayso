//! Asks the operating system to treat speech recognition as foreground work.
//!
//! The core is a windowless child process. On Windows 11 with a hybrid CPU (fast
//! "performance" cores plus slow "efficiency" cores), such a process is a candidate for
//! power throttling: its threads can be parked on the efficiency cores, where recognition
//! runs two to three times slower. These calls opt out.

/// Opts the process out of power throttling and raises its priority a notch.
/// Returns whether the request was accepted.
#[cfg(windows)]
pub fn request_full_speed() -> bool {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
        ABOVE_NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    };
    unsafe {
        let process = GetCurrentProcess();
        // ControlMask says which behaviours we are setting; a zero StateMask turns them off.
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        let throttling = SetProcessInformation(
            process,
            ProcessPowerThrottling,
            std::ptr::addr_of!(state).cast(),
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
        let priority = SetPriorityClass(process, ABOVE_NORMAL_PRIORITY_CLASS);
        throttling != 0 && priority != 0
    }
}

#[cfg(not(windows))]
pub fn request_full_speed() -> bool {
    false
}
