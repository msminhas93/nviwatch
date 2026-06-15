use crate::gpu::process::GpuProcessInfo;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use nix::unistd::{sysconf, SysconfVar};
use nix::unistd::{Uid, User};
use procfs::process::Process;
use std::fs;

pub fn get_process_info(pid: u32, used_gpu_memory: u64) -> Option<GpuProcessInfo> {
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
                        (total_time as f64 / clock_ticks as f64 / uptime * 100.0) as f32
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

pub fn kill_selected_process(
    pid: u32,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        Ok(_) => Ok(()),
        Err(nix::Error::EPERM) => Err(format!(
            "Permission denied to terminate process {pid} ({command})"
        )
        .into()),
        Err(e) => Err(format!(
            "Failed to terminate process {pid} ({command}): {e}"
        )
        .into()),
    }
}

pub fn get_clock_ticks_per_second() -> u64 {
    sysconf(SysconfVar::CLK_TCK)
        .unwrap()
        .map(|ticks| ticks as u64)
        .unwrap_or(100)
}

pub fn get_system_uptime() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|content| content.split_whitespace().next().map(String::from))
        .and_then(|uptime_str| uptime_str.parse().ok())
        .unwrap_or(0.0)
}
