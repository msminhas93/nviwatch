use crate::gpu::process::GpuProcessInfo;
// use crate::platform::{get_process_manager, get_system_info, ProcessInfo};
use crate::AppState;
use std::io::{Error as IoError, ErrorKind};

#[cfg(target_os = "linux")]
use nix::sys::signal::{kill, Signal};
#[cfg(target_os = "linux")]
use nix::unistd::Pid;
#[cfg(target_os = "linux")]
use nix::unistd::{sysconf, SysconfVar};
#[cfg(target_os = "linux")]
use nix::unistd::{Uid, User};
#[cfg(target_os = "linux")]
use procfs::process::Process;
#[cfg(target_os = "linux")]
use std::fs;

pub fn get_process_info(pid: u32, used_gpu_memory: u64) -> Option<GpuProcessInfo> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(process) = Process::new(pid as i32) {
            if let Ok(uid) = process.uid() {
                if let Ok(Some(user)) = User::from_uid(Uid::from_raw(uid)) {
                    let command = process.cmdline().unwrap_or_default().join(" ");
                    let cpu_usage = process
                        .stat()
                        .ok()
                        .map(|stat| {
                            let total_time = stat.utime + stat.stime;
                            let clock_ticks = get_clock_ticks_per_second();
                            let uptime = get_system_uptime();
                            if uptime > 0.0 {
                                (total_time as f64 / clock_ticks as f64 / uptime * 100.0) as f32
                            } else {
                                0.0
                            }
                        })
                        .unwrap_or(0.0);
                    let memory_usage = process.stat().ok().map(|stat| stat.rss * 4096).unwrap_or(0);

                    return Some(GpuProcessInfo {
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

    #[cfg(not(target_os = "linux"))]
    {
        // Fallback for non-Linux platforms
        Some(GpuProcessInfo {
            pid,
            used_gpu_memory,
            username: "Unknown".to_string(),
            command: format!("Process {}", pid),
            cpu_usage: 0.0,
            memory_usage: 0,
        })
    }
}

pub fn kill_selected_process(app_state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_processes = Vec::new();
    for gpu_info in &app_state.gpu_infos {
        all_processes.extend(gpu_info.processes.iter());
    }

    // Sort processes by GPU memory usage (descending) to match the UI
    all_processes.sort_by(|a, b| b.used_gpu_memory.cmp(&a.used_gpu_memory));

    if app_state.selected_process < all_processes.len() {
        let selected_process = &all_processes[app_state.selected_process];
        let pid = selected_process.pid;

        #[cfg(target_os = "linux")]
        {
            match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                Ok(_) => Ok(()),
                Err(nix::Error::EPERM) => Err(Box::new(IoError::new(
                    ErrorKind::PermissionDenied,
                    format!(
                        "Permission denied to terminate process {} ({})",
                        pid, selected_process.command
                    ),
                ))),
                Err(e) => Err(Box::new(IoError::new(
                    ErrorKind::Other,
                    format!(
                        "Failed to terminate process {} ({}): {}",
                        pid, selected_process.command, e
                    ),
                ))),
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(Box::new(IoError::new(
                ErrorKind::Other,
                "Process termination not supported on this platform",
            )))
        }
    } else {
        Err(Box::new(IoError::new(
            ErrorKind::NotFound,
            "Selected process not found",
        )))
    }
}

pub fn get_clock_ticks_per_second() -> u64 {
    #[cfg(target_os = "linux")]
    {
        sysconf(SysconfVar::CLK_TCK)
            .unwrap()
            .map(|ticks| ticks as u64)
            .unwrap_or(100)
    }

    #[cfg(not(target_os = "linux"))]
    {
        100 // Default fallback
    }
}

pub fn get_system_uptime() -> f64 {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|content| content.split_whitespace().next().map(String::from))
            .and_then(|uptime_str| uptime_str.parse().ok())
            .unwrap_or(0.0)
    }

    #[cfg(not(target_os = "linux"))]
    {
        0.0 // Default fallback
    }
}
