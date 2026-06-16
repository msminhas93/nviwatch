use crate::AppState;
use crate::gpu::GpuProcessInfo;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use nix::unistd::{SysconfVar, sysconf};
use nix::unistd::{Uid, User};
use procfs::process::Process;
use std::io::{Error as IoError, ErrorKind};

pub fn get_process_info(pid: u32, used_gpu_memory: u64) -> Option<GpuProcessInfo> {
    let process = Process::new(pid as i32).ok()?;
    let uid = process.uid().ok()?;
    let user = User::from_uid(Uid::from_raw(uid)).ok().flatten()?;

    let command = process.cmdline().unwrap_or_default().join(" ");

    let stat = process.stat().ok()?;
    let total_time = stat.utime + stat.stime;
    let clock_ticks = get_clock_ticks_per_second();
    let uptime = get_system_uptime();
    let cpu_usage = (total_time as f64 / clock_ticks as f64 / uptime * 100.0) as f32;
    let memory_usage = stat.rss * 4096;

    Some(GpuProcessInfo {
        pid,
        used_gpu_memory,
        username: user.name,
        command,
        cpu_usage,
        memory_usage,
    })
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
    } else {
        Err(Box::new(IoError::new(
            ErrorKind::NotFound,
            "Selected process not found",
        )))
    }
}

pub fn get_clock_ticks_per_second() -> u64 {
    sysconf(SysconfVar::CLK_TCK)
        .ok()
        .flatten()
        .map(|ticks| ticks as u64)
        .unwrap_or(100)
}

pub fn get_system_uptime() -> f64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|content| content.split_whitespace().next().map(String::from))
        .and_then(|uptime_str| uptime_str.parse().ok())
        .unwrap_or(0.0)
}
