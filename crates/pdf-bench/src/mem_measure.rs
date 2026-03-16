/// Memory measurement helper — macOS only (uses task_info via sysctl workaround).
/// Falls back to getrusage ru_maxrss on other platforms.

/// Returns current physical memory footprint in bytes for this process.
#[cfg(target_os = "macos")]
pub fn physical_footprint() -> u64 {
    use std::mem;
    // Use mach task_info to get the current physical memory footprint.
    // MACH_TASK_BASIC_INFO gives resident_size (current RSS).
    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut std::ffi::c_void,
            task_info_outCnt: *mut u32,
        ) -> i32;
    }

    // MACH_TASK_BASIC_INFO = 20, count = 12 words
    const MACH_TASK_BASIC_INFO: u32 = 20;
    const MACH_TASK_BASIC_INFO_COUNT: u32 = 12;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [u32; 2],
        system_time: [u32; 2],
        policy: i32,
        suspend_count: i32,
    }

    let mut info: MachTaskBasicInfo = unsafe { mem::zeroed() };
    let mut count = MACH_TASK_BASIC_INFO_COUNT;

    unsafe {
        task_info(
            mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info as *mut _ as *mut std::ffi::c_void,
            &mut count,
        );
    }
    info.resident_size
}

#[cfg(not(target_os = "macos"))]
pub fn physical_footprint() -> u64 {
    // Linux: read VmRSS from /proc/self/status
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let kb: u64 = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                return kb * 1024;
            }
        }
    }
    0
}
