use super::SystemInfo;

#[cfg(target_os = "linux")]
pub struct LinuxSystemInfo;

#[cfg(target_os = "linux")]
impl SystemInfo for LinuxSystemInfo {
    fn get_clock_ticks_per_second(&self) -> u64 {
        use nix::unistd::sysconf;
        use nix::unistd::SysconfVar;
        
        sysconf(SysconfVar::CLK_TCK)
            .unwrap()
            .map(|ticks| ticks as u64)
            .unwrap_or(100)
    }

    fn get_system_uptime(&self) -> f64 {
        use std::fs;
        
        fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|content| content.split_whitespace().next().map(String::from))
            .and_then(|uptime_str| uptime_str.parse().ok())
            .unwrap_or(0.0)
    }
}

#[cfg(target_os = "windows")]
pub struct WindowsSystemInfo;

#[cfg(target_os = "windows")]
impl SystemInfo for WindowsSystemInfo {
    fn get_clock_ticks_per_second(&self) -> u64 {
        // Windows typically uses 1000 ticks per second
        1000
    }

    fn get_system_uptime(&self) -> f64 {
        use windows::Win32::System::SystemInformation::GetTickCount64;
        
        unsafe {
            let ticks = GetTickCount64();
            ticks as f64 / 1000.0 // Convert milliseconds to seconds
        }
    }
}

#[cfg(target_os = "macos")]
pub struct MacOSSystemInfo;

#[cfg(target_os = "macos")]
impl SystemInfo for MacOSSystemInfo {
    fn get_clock_ticks_per_second(&self) -> u64 {
        // macOS typically uses 100 ticks per second
        100
    }

    fn get_system_uptime(&self) -> f64 {
        use libc::sysctl;
        use std::ffi::CString;
        
        unsafe {
            let mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
            let mut boottime: libc::timeval = std::mem::zeroed();
            let mut size = std::mem::size_of::<libc::timeval>();
            
            if sysctl(mib.as_ptr(), 2, &mut boottime as *mut _ as *mut libc::c_void, &mut size, std::ptr::null_mut(), 0) == 0 {
                let now = libc::time(std::ptr::null_mut());
                (now - boottime.tv_sec) as f64
            } else {
                0.0
            }
        }
    }
}

// Fallback implementation for unsupported platforms
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub struct UnsupportedSystemInfo;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
impl SystemInfo for UnsupportedSystemInfo {
    fn get_clock_ticks_per_second(&self) -> u64 {
        100 // Default fallback
    }

    fn get_system_uptime(&self) -> f64 {
        0.0 // Default fallback
    }
}
