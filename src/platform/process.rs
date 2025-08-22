use super::{ProcessInfo, ProcessManager};
use std::error::Error;

#[cfg(target_os = "linux")]
pub struct LinuxProcessManager;

#[cfg(target_os = "linux")]
impl ProcessManager for LinuxProcessManager {
    fn kill_process(&self, pid: u32) -> Result<(), Box<dyn Error>> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)?;
        Ok(())
    }

    fn get_process_info(&self, pid: u32, used_gpu_memory: u64) -> Option<ProcessInfo> {
        use nix::unistd::{Uid, User};
        use procfs::process::Process;

        if let Ok(process) = Process::new(pid as i32) {
            if let Ok(uid) = process.uid() {
                if let Ok(Some(user)) = User::from_uid(Uid::from_raw(uid)) {
                    let command = process.cmdline().unwrap_or_default().join(" ");
                    let cpu_usage = process
                        .stat()
                        .ok()
                        .map(|stat| {
                            let total_time = stat.utime + stat.stime;
                            let clock_ticks =
                                crate::platform::get_system_info().get_clock_ticks_per_second();
                            let uptime = crate::platform::get_system_info().get_system_uptime();
                            if uptime > 0.0 {
                                (total_time as f64 / clock_ticks as f64 / uptime * 100.0) as f32
                            } else {
                                0.0
                            }
                        })
                        .unwrap_or(0.0);
                    let memory_usage = process.stat().ok().map(|stat| stat.rss * 4096).unwrap_or(0);

                    return Some(ProcessInfo {
                        pid,
                        used_gpu_memory,
                        username: user.name,
                        command,
                        cpu_usage,
                        memory_usage,
                    });
                }
            }
        }
        None
    }
}

#[cfg(target_os = "windows")]
pub struct WindowsProcessManager;

#[cfg(target_os = "windows")]
impl ProcessManager for WindowsProcessManager {
    fn kill_process(&self, pid: u32) -> Result<(), Box<dyn Error>> {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, false, pid)?;
            TerminateProcess(handle, 0)?;
            CloseHandle(handle);
        }
        Ok(())
    }

    fn get_process_info(&self, pid: u32, used_gpu_memory: u64) -> Option<ProcessInfo> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::ProcessStatus::GetProcessMemoryInfo;
        use windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS;
        use windows::Win32::System::SystemInformation::GetTickCount64;
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        };

        unsafe {
            let handle =
                OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;

            // Get memory info
            let mut mem_counters = PROCESS_MEMORY_COUNTERS::default();
            if GetProcessMemoryInfo(
                handle,
                &mut mem_counters,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
            .is_ok()
            {
                let memory_usage = mem_counters.WorkingSetSize;

                // Get process name (simplified - in production you'd want more robust process name extraction)
                let command = format!("Process {}", pid);

                // Get username (simplified - would need additional Windows API calls for full user info)
                let username = "Unknown".to_string();

                // CPU usage calculation (simplified - would need more complex Windows performance counters)
                let cpu_usage = 0.0;

                CloseHandle(handle);

                return Some(ProcessInfo {
                    pid,
                    used_gpu_memory,
                    username,
                    command,
                    cpu_usage,
                    memory_usage: memory_usage as u64,
                });
            }

            CloseHandle(handle);
        }
        None
    }
}

#[cfg(target_os = "macos")]
pub struct MacOSProcessManager;

#[cfg(target_os = "macos")]
impl ProcessManager for MacOSProcessManager {
    fn kill_process(&self, pid: u32) -> Result<(), Box<dyn Error>> {
        use libc::{kill, SIGTERM};

        let result = unsafe { kill(pid as i32, SIGTERM) };
        if result == 0 {
            Ok(())
        } else {
            Err(format!(
                "Failed to kill process {}: {}",
                pid,
                std::io::Error::last_os_error()
            )
            .into())
        }
    }

    fn get_process_info(&self, pid: u32, used_gpu_memory: u64) -> Option<ProcessInfo> {
        use libc::{proc_pidinfo, PROC_PIDPATHINFO_MAXSIZE, PROC_PIDTASKINFO};
        use std::ffi::CString;
        use std::os::raw::c_char;

        unsafe {
            // Get task info
            let mut task_info: libc::proc_taskinfo = std::mem::zeroed();
            let result = proc_pidinfo(
                pid as i32,
                PROC_PIDTASKINFO,
                0,
                &mut task_info as *mut _ as *mut libc::c_void,
                std::mem::size_of::<libc::proc_taskinfo>(),
            );

            if result == std::mem::size_of::<libc::proc_taskinfo>() as i32 {
                // Get process path
                let mut path_buf = vec![0u8; PROC_PIDPATHINFO_MAXSIZE as usize];
                let path_result = proc_pidinfo(
                    pid as i32,
                    libc::PROC_PIDPATHINFO,
                    0,
                    path_buf.as_mut_ptr() as *mut libc::c_void,
                    PROC_PIDPATHINFO_MAXSIZE,
                );

                let command = if path_result > 0 {
                    let path_len = path_result as usize;
                    String::from_utf8_lossy(&path_buf[..path_len]).to_string()
                } else {
                    format!("Process {}", pid)
                };

                // Simplified user info (would need additional macOS API calls for full user info)
                let username = "Unknown".to_string();

                // CPU usage calculation (simplified)
                let cpu_usage = 0.0;

                return Some(ProcessInfo {
                    pid,
                    used_gpu_memory,
                    username,
                    command,
                    cpu_usage,
                    memory_usage: task_info.pti_resident_size as u64,
                });
            }
        }
        None
    }
}

// Fallback implementation for unsupported platforms
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub struct UnsupportedProcessManager;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
impl ProcessManager for UnsupportedProcessManager {
    fn kill_process(&self, _pid: u32) -> Result<(), Box<dyn Error>> {
        Err("Process management not supported on this platform".into())
    }

    fn get_process_info(&self, _pid: u32, _used_gpu_memory: u64) -> Option<ProcessInfo> {
        None
    }
}
