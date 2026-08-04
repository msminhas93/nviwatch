use crate::error::NviError;
use crate::gpu::GpuProcessInfo;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use nix::unistd::{SysconfVar, sysconf};
use nix::unistd::{Uid, User};
use procfs::process::Process;
use std::time::Instant;

/// Prior `/proc/[pid]/stat` CPU tick sample for delta %CPU.
#[derive(Clone, Copy, Debug)]
pub struct CpuSample {
    pub total_ticks: u64,
    pub at: Instant,
}

pub fn get_process_info(
    pid: u32,
    used_gpu_memory: u64,
    previous: Option<&CpuSample>,
) -> Option<(GpuProcessInfo, CpuSample)> {
    let process = Process::new(pid as i32).ok()?;
    let uid = process.uid().ok()?;
    let user = User::from_uid(Uid::from_raw(uid)).ok().flatten()?;

    let command = process.cmdline().unwrap_or_default().join(" ");

    let stat = process.stat().ok()?;
    let total_ticks = stat.utime + stat.stime;
    let clock_ticks = get_clock_ticks_per_second() as f64;
    let memory_usage = stat.rss * 4096;

    // Top-style %CPU: CPU seconds accrued / wall seconds since last sample × 100.
    // Can exceed 100% on multi-core. Unavailable until a second sample exists.
    let now = Instant::now();
    let cpu_percent = previous.and_then(|prev| {
        let dt = now.duration_since(prev.at).as_secs_f64();
        if dt <= f64::EPSILON || total_ticks < prev.total_ticks {
            return None;
        }
        let delta_secs = (total_ticks - prev.total_ticks) as f64 / clock_ticks;
        Some((delta_secs / dt * 100.0) as f32)
    });

    let sample = CpuSample {
        total_ticks,
        at: now,
    };

    Some((
        GpuProcessInfo {
            pid,
            used_gpu_memory,
            username: user.name,
            command,
            cpu_percent,
            memory_usage,
        },
        sample,
    ))
}

pub fn kill_selected_process(pid: u32, command: &str) -> Result<(), NviError> {
    match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        Ok(_) => Ok(()),
        Err(nix::Error::EPERM) => Err(NviError::Process(format!(
            "Permission denied to terminate process {pid} ({command})"
        ))),
        Err(e) => Err(NviError::Process(format!(
            "Failed to terminate process {pid} ({command}): {e}"
        ))),
    }
}

pub fn get_clock_ticks_per_second() -> u64 {
    sysconf(SysconfVar::CLK_TCK)
        .ok()
        .flatten()
        .map(|ticks| ticks as u64)
        .unwrap_or(100)
}
