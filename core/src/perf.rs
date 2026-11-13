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

/// Restricts the process to the CPU's performance cores, if it has two kinds of core.
/// Returns how many logical processors the process may now use, or `None` if nothing changed.
#[cfg(windows)]
pub fn use_performance_cores_only() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessAffinityMask};
    unsafe {
        let process = GetCurrentProcess();
        let mut needed = 0u32;
        GetSystemCpuSetInformation(std::ptr::null_mut(), 0, &mut needed, process, 0);
        if needed == 0 {
            return None;
        }
        let mut buffer = vec![0u8; needed as usize];
        if GetSystemCpuSetInformation(buffer.as_mut_ptr().cast(), needed, &mut needed, process, 0) == 0 {
            return None;
        }
        // The buffer is a sequence of variable-sized records.
        let mut cores: Vec<(u8, u8)> = Vec::new(); // (logical processor index, efficiency class)
        let mut offset = 0usize;
        while offset + std::mem::size_of::<SYSTEM_CPU_SET_INFORMATION>() <= needed as usize {
            let info = &*(buffer.as_ptr().add(offset) as *const SYSTEM_CPU_SET_INFORMATION);
            let set = info.Anonymous.CpuSet;
            if set.Group == 0 {
                cores.push((set.LogicalProcessorIndex, set.EfficiencyClass));
            }
            if info.Size == 0 {
                break;
            }
            offset += info.Size as usize;
        }
        let fastest = cores.iter().map(|c| c.1).max()?;
        if cores.iter().all(|c| c.1 == fastest) {
            return None; // one kind of core: nothing to choose between
        }
        let mask = cores
            .iter()
            .filter(|c| c.1 == fastest && c.0 < usize::BITS as u8)
            .fold(0usize, |mask, c| mask | (1usize << c.0));
        if mask == 0 || SetProcessAffinityMask(process, mask) == 0 {
            return None;
        }
        Some(mask.count_ones() as usize)
    }
}

#[cfg(not(windows))]
pub fn use_performance_cores_only() -> Option<usize> {
    None
}
