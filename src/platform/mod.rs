pub mod process;
pub mod system;

use std::error::Error;

/// Platform-agnostic process information
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub used_gpu_memory: u64,
    pub username: String,
    pub command: String,
    pub cpu_usage: f32,
    pub memory_usage: u64,
}

/// Platform-agnostic process manager trait
pub trait ProcessManager {
    fn kill_process(&self, pid: u32) -> Result<(), Box<dyn Error>>;
    fn get_process_info(&self, pid: u32, used_gpu_memory: u64) -> Option<ProcessInfo>;
}

/// Platform-agnostic system information trait
pub trait SystemInfo {
    fn get_clock_ticks_per_second(&self) -> u64;
    fn get_system_uptime(&self) -> f64;
}

/// Get the appropriate process manager for the current platform
pub fn get_process_manager() -> Box<dyn ProcessManager> {
    #[cfg(target_os = "linux")]
    return Box::new(process::LinuxProcessManager);

    #[cfg(target_os = "windows")]
    return Box::new(process::WindowsProcessManager);

    #[cfg(target_os = "macos")]
    return Box::new(process::MacOSProcessManager);

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    compile_error!("Unsupported operating system");
}

/// Get the appropriate system info provider for the current platform
pub fn get_system_info() -> Box<dyn SystemInfo> {
    #[cfg(target_os = "linux")]
    return Box::new(system::LinuxSystemInfo);

    #[cfg(target_os = "windows")]
    return Box::new(system::WindowsSystemInfo);

    #[cfg(target_os = "macos")]
    return Box::new(system::MacOSSystemInfo);

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    compile_error!("Unsupported operating system");
}
