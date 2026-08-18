use crate::error::NviError;
use crate::gpu::GpuProcessInfo;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use nix::unistd::{sysconf, SysconfVar};
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

    // Instant % of total machine CPU (0–100). Unavailable until a second sample.
    // Top-style (cpu_secs/wall_secs*100) is unbounded and hits 700% on 7 busy
    // cores — that is the main-branch bug this replaces.
    let now = Instant::now();
    let ncpus = online_cpus();
    let cpu_percent = previous.and_then(|prev| {
        if total_ticks < prev.total_ticks {
            return None;
        }
        let dt = now.duration_since(prev.at).as_secs_f64();
        instant_process_cpu_pct(total_ticks - prev.total_ticks, clock_ticks, dt, ncpus)
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

pub fn online_cpus() -> u32 {
    sysconf(SysconfVar::_NPROCESSORS_ONLN)
        .ok()
        .flatten()
        .map(|n| n as u32)
        .filter(|&n| n > 0)
        .unwrap_or(1)
}

/// Instant process CPU as a percent of total machine capacity (0–100).
///
/// `delta_ticks` is utime+stime accrued since the last sample. `dt_secs` is
/// wall time between samples. `ncpus` is online logical CPUs.
///
/// htop-style %CPU omits `/ ncpus` and reports 800% on an 8-core box. A
/// `CPU%` column must not do that.
pub fn instant_process_cpu_pct(
    delta_ticks: u64,
    clock_ticks: f64,
    dt_secs: f64,
    ncpus: u32,
) -> Option<f32> {
    if dt_secs <= f64::EPSILON || clock_ticks <= 0.0 || ncpus == 0 {
        return None;
    }
    let cpu_secs = delta_ticks as f64 / clock_ticks;
    let pct = cpu_secs / dt_secs / f64::from(ncpus) * 100.0;
    Some(pct.clamp(0.0, 100.0) as f32)
}

/// Same metric from two tick counts in the same unit (process vs `/proc/stat`
/// aggregate `cpu` line). Do not multiply by core count — `system_delta`
/// already spans every CPU.
pub fn instant_process_cpu_pct_from_system(proc_delta: u64, system_delta: u64) -> f32 {
    if system_delta == 0 {
        0.0
    } else {
        (proc_delta as f64 / system_delta as f64 * 100.0).clamp(0.0, 100.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 7 cores busy for 1s on an 8-core box @ 100 Hz.
    // Top-style (main today) reports 700%. Column is CPU% of the machine.
    const PROC_TICKS: u64 = 700;
    const CLOCK: f64 = 100.0;
    const DT: f64 = 1.0;
    const NCPUS: u32 = 8;
    const SYSTEM_TICKS: u64 = 800;

    #[test]
    fn seven_of_eight_cores_is_87_5_not_700() {
        let pct = instant_process_cpu_pct(PROC_TICKS, CLOCK, DT, NCPUS).unwrap();
        assert!(
            (pct - 87.5).abs() < 0.01,
            "expected 87.5% of machine, got {pct} (top-style would be 700)"
        );
        assert!(pct <= 100.0, "CPU% column must never exceed 100, got {pct}");
    }

    #[test]
    fn system_delta_path_matches_wall_clock_path() {
        let wall = instant_process_cpu_pct(PROC_TICKS, CLOCK, DT, NCPUS).unwrap();
        let sys = instant_process_cpu_pct_from_system(PROC_TICKS, SYSTEM_TICKS);
        assert!((wall - sys).abs() < 0.01, "wall={wall} system={sys}");
    }

    #[test]
    fn fully_busy_machine_is_100_not_800() {
        let pct = instant_process_cpu_pct(800, CLOCK, DT, NCPUS).unwrap();
        assert!((pct - 100.0).abs() < 0.01, "got {pct}");
        let sys = instant_process_cpu_pct_from_system(800, 800);
        assert!((sys - 100.0).abs() < 0.01, "got {sys}");
    }

    #[test]
    fn sampling_skew_clamps_to_100() {
        // Process ticks can briefly exceed the /proc/stat aggregate window.
        let pct = instant_process_cpu_pct_from_system(900, 800);
        assert_eq!(pct, 100.0);
    }

    #[test]
    fn no_elapsed_time_is_unavailable() {
        assert_eq!(instant_process_cpu_pct(700, CLOCK, 0.0, NCPUS), None);
        assert_eq!(instant_process_cpu_pct_from_system(700, 0), 0.0);
    }
}
