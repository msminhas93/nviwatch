use std::time::Instant;

/// Prior `/proc/[pid]/stat` CPU tick sample for delta %CPU.
///
/// On Unix `total_ticks` is the cumulative `utime + stime` from `/proc`.
/// On Windows there is no cumulative tick counter, so `total_ticks` stores a
/// sysinfo-derived value instead (see the Windows backend for the convention).
/// The struct's public contract — a `u64` carried between ticks with a timestamp —
/// is unchanged, and callers (`gpu/info.rs`) treat it opaquely.
#[derive(Clone, Copy, Debug)]
pub struct CpuSample {
    pub total_ticks: u64,
    pub at: Instant,
}

#[cfg(unix)]
mod imp {
    use super::CpuSample;
    use crate::error::NviError;
    use crate::gpu::GpuProcessInfo;
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    use nix::unistd::{SysconfVar, sysconf};
    use nix::unistd::{Uid, User};
    use procfs::process::Process;
    use std::time::Instant;

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
}

#[cfg(windows)]
mod imp {
    use super::CpuSample;
    use crate::error::NviError;
    use crate::gpu::GpuProcessInfo;
    use std::cell::RefCell;
    use sysinfo::{Pid, ProcessesToUpdate, Signal, System, Users};

    /// Shared sysinfo probe: `System` for process/CPU/memory data, `Users` for
    /// username lookup. Held in a `thread_local!` so the public signatures of
    /// `get_process_info` / `collect_system_stats` stay unchanged (the whole app
    /// is single-threaded for these calls). Constructing a `System::new_all()`
    /// per tick would re-enumerate every process per GPU process per refresh —
    /// a full scan ~13×/s on a 4-GPU-proc box at `--watch 300`, which would sink
    /// the "0.28% CPU" headline number. The probe refreshes only what is needed.
    pub struct PlatformProbe {
        // `pub(crate)`: the `--cpu` backend in `system_monitor::imp` drives the
        // same shared `System`/`Users` directly rather than through wrappers, so
        // one refresh covers both the GPU-process and system-stat paths.
        pub(crate) system: System,
        pub(crate) users: Users,
        // Whether `users` has been populated yet. `Users::new()` is empty by
        // design (see `ensure_users`); the LSA-backed enumeration runs lazily on
        // the first real production lookup, not at construction. This keeps the
        // pure-logic unit tests (which drive an empty `System`) off the live
        // Windows backend entirely.
        users_refreshed: bool,
    }

    impl PlatformProbe {
        fn new() -> Self {
            Self {
                system: System::new(),
                users: Users::new(),
                users_refreshed: false,
            }
        }

        /// Populate the user table once per probe lifetime. Logged-in users
        /// barely change during a session, and `Users::refresh()` is the one
        /// sysinfo call that touches the Windows LSA (`LsaEnumerateLogonSessions`)
        /// — deferring it (instead of running it at construction, as
        /// `Users::new_with_refreshed_list` would) keeps username lookup
        /// pay-per-use and the unit tests off the live system backend.
        pub(crate) fn ensure_users(&mut self) {
            if !self.users_refreshed {
                self.users.refresh();
                self.users_refreshed = true;
            }
        }
    }

    thread_local! {
        static PROBE: RefCell<PlatformProbe> = RefCell::new(PlatformProbe::new());
    }

    /// Run `f` against the thread-local probe. Exposed for `system_monitor`'s
    /// `--cpu` backend, which needs the same shared `System` instance.
    pub(crate) fn with_probe<R>(f: impl FnOnce(&mut PlatformProbe) -> R) -> R {
        PROBE.with_borrow_mut(f)
    }

    pub fn get_process_info(
        pid: u32,
        used_gpu_memory: u64,
        previous: Option<&CpuSample>,
    ) -> Option<(GpuProcessInfo, CpuSample)> {
        with_probe(|probe| {
            let sid = Pid::from_u32(pid);
            // Refresh only this PID: avoids a full process enumeration per GPU
            // process per tick (the perf killer called out in the spec).
            probe
                .system
                .refresh_processes(ProcessesToUpdate::Some(&[sid]), false);
            // Populate the user table once so username lookup below can resolve.
            probe.ensure_users();
            let process = probe.system.process(sid)?;

            let username = process
                .user_id()
                .and_then(|uid| probe.users.get_user_by_id(uid))
                .map(|u| u.name().to_string())
                .unwrap_or_else(|| "?".to_string());

            let command = {
                let joined = process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !joined.is_empty() {
                    joined
                } else {
                    let name = process.name().to_string_lossy().into_owned();
                    if name.is_empty() {
                        format!("[{pid}]")
                    } else {
                        name
                    }
                }
            };

            // sysinfo >= 0.30 returns RSS in bytes (no page-size multiply needed).
            let memory_usage = process.memory();

            // sysinfo's `cpu_usage()` is top-style: 100 = one saturated core,
            // can exceed 100% on multi-core — same convention as the unix /proc
            // calculation, so no normalisation is applied. It needs two refreshes
            // to be meaningful; gate on `previous` so the first sample shows "—"
            // (None) exactly like the unix backend's first-tick behaviour.
            let raw = process.cpu_usage();
            let cpu_percent = previous.map(|_| raw);

            // `total_ticks` has no cumulative-tick meaning on Windows; stash the
            // scaled percentage so the struct contract (`u64` + timestamp) holds.
            // The value is opaque to callers and is not used for delta math here
            // because sysinfo already computes the delta internally.
            let sample = CpuSample {
                total_ticks: (raw as f64 * 100.0) as u64,
                at: std::time::Instant::now(),
            };

            Some((
                GpuProcessInfo {
                    pid,
                    used_gpu_memory,
                    username,
                    command,
                    cpu_percent,
                    memory_usage,
                },
                sample,
            ))
        })
    }

    pub fn kill_selected_process(pid: u32, command: &str) -> Result<(), NviError> {
        with_probe(|probe| {
            let sid = Pid::from_u32(pid);
            probe
                .system
                .refresh_processes(ProcessesToUpdate::Some(&[sid]), false);
            let Some(process) = probe.system.process(sid) else {
                return Err(NviError::Process(format!(
                    "Failed to terminate process {pid} ({command}): no such process"
                )));
            };
            // Try a graceful terminate first; sysinfo returns `None` when the
            // signal is unsupported on this platform (Windows supports few), in
            // which case fall back to the always-available force-kill. sysinfo
            // exposes only a `bool`, so — unlike the unix EPERM branch — we cannot
            // distinguish permission denial from other failures; a failed send is
            // reported with the generic message used by the unix non-EPERM path.
            let sent = process
                .kill_with(Signal::Term)
                .unwrap_or_else(|| process.kill());
            if sent {
                Ok(())
            } else {
                Err(NviError::Process(format!(
                    "Failed to terminate process {pid} ({command})"
                )))
            }
        })
    }
}

pub use imp::*;
