//! System-wide CPU / memory / process scanning used by `--cpu` mode.

use crate::app_state::AppState;
use crate::error::NviError;
use crate::gpu::info::GpuInfo;
use nix::unistd::{Uid, User};
use procfs::prelude::*;
use procfs::process::{all_processes, Process};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Number of processes kept in the bottom tray. We scan every process cheaply
/// (one /proc/<pid>/stat read each) to compute CPU%, but only the busiest
/// `TOP_N` are enriched with username + cmdline and rendered.
const TOP_N: usize = 50;

/// Width of a per-core meter bar in characters.
const BAR_WIDTH: usize = 6;

/// Max samples for the aggregate CPU% history graph (mirrors GPU history).
const HISTORY_CAP: usize = 60;

/// How the system process tray is ordered.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortMode {
    #[default]
    Cpu,
    GpuMemory,
}

/// Cumulative CPU-time snapshot for deriving a percentage from the delta between ticks.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct KernelCpuSample {
    pub total: u64,
    pub idle: u64,
}

/// System-wide CPU + memory snapshot for the left-hand panels.
#[derive(Default, Clone)]
pub struct CpuStats {
    pub model: String,
    pub logical_cores: usize,
    pub per_core_usage: Vec<f32>,
    pub aggregate_usage: f32,
    pub frequency_mhz: f64,
    pub load_avg: (f32, f32, f32),
    pub mem_total: u64,
    pub mem_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// A unified process row for the `--cpu` bottom tray.
#[derive(Clone, Debug)]
pub struct SystemProcess {
    pub pid: i32,
    pub gpu_memory: Option<u64>,
    pub gpu_index: Option<usize>,
    pub username: String,
    pub command: String,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub state: char,
}

/// Sort a process slice in place by the active mode.
pub fn sort_processes(procs: &mut [SystemProcess], mode: SortMode) {
    // PID is a stable tie-breaker so near-idle rows don't reshuffle every tick
    // when CPU% is equal (or both round to the same display value).
    match mode {
        SortMode::Cpu => procs.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.pid.cmp(&b.pid))
        }),
        SortMode::GpuMemory => procs.sort_by(|a, b| {
            b.gpu_memory
                .unwrap_or(0)
                .cmp(&a.gpu_memory.unwrap_or(0))
                .then_with(|| {
                    b.cpu_usage
                        .partial_cmp(&a.cpu_usage)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.pid.cmp(&b.pid))
        }),
    }
}

/// Width (in chars) of one per-core meter cell.
pub fn meter_cell_width() -> usize {
    // " 12[||||  ] 88% " -> 3 + 1 + BAR_WIDTH + 1 + 4 + 1
    3 + 1 + BAR_WIDTH + 1 + 4 + 1
}

/// Build the filled/empty bar string for a single core meter.
pub fn meter_bar(pct: f32) -> (String, String) {
    let filled = ((pct / 100.0) * BAR_WIDTH as f32).round() as usize;
    let filled = filled.min(BAR_WIDTH);
    ("|".repeat(filled), " ".repeat(BAR_WIDTH - filled))
}

/// Collect CPU stats, system memory, and the top-N process list.
pub fn collect_system_stats(app_state: &mut AppState) -> Result<(), NviError> {
    let ks = procfs::KernelStats::current().map_err(|e| NviError::General(e.to_string()))?;
    let cur_total = kernel_cpu_sample(&ks.total);
    let total_delta = app_state
        .prev_cpu_total
        .as_ref()
        .map(|p| cur_total.total.saturating_sub(p.total))
        .unwrap_or(0);
    let aggregate_usage = app_state
        .prev_cpu_total
        .as_ref()
        .map(|p| pct(p, &cur_total))
        .unwrap_or(0.0);
    let logical_cores = ks.cpu_time.len();
    let cur_per_core: Vec<KernelCpuSample> = ks.cpu_time.iter().map(kernel_cpu_sample).collect();
    let per_core_usage: Vec<f32> = cur_per_core
        .iter()
        .enumerate()
        .map(|(i, cur)| {
            app_state
                .prev_cpu_per_core
                .get(i)
                .map(|p| pct(p, cur))
                .unwrap_or(0.0)
        })
        .collect();

    let (model, frequency_mhz) =
        read_cpu_model_freq().unwrap_or_else(|| ("Unknown CPU".to_string(), 0.0));
    let load_avg = procfs::LoadAverage::current()
        .map(|l| (l.one, l.five, l.fifteen))
        .unwrap_or((0.0, 0.0, 0.0));
    let mem = procfs::Meminfo::current().map_err(|e| NviError::General(e.to_string()))?;
    let mem_total = mem.mem_total;
    let mem_used = mem_total.saturating_sub(mem.mem_available.unwrap_or(mem.mem_free));
    let swap_total = mem.swap_total;
    let swap_used = swap_total.saturating_sub(mem.swap_free);

    let gpu_map = build_gpu_map(&app_state.gpu_infos);
    let page = procfs::page_size();
    let mut procs: Vec<SystemProcess> = Vec::new();
    let mut new_prev: HashMap<i32, u64> = HashMap::new();

    if let Ok(iter) = all_processes() {
        for p in iter.filter_map(|p| p.ok()) {
            if let Ok(stat) = p.stat() {
                let pid = stat.pid;
                let ticks = stat.utime + stat.stime;
                new_prev.insert(pid, ticks);

                let cpu_usage = if total_delta > 0 {
                    match app_state.prev_proc_times.get(&pid) {
                        Some(&prev) => {
                            (ticks.saturating_sub(prev) as f64 / total_delta as f64
                                * logical_cores as f64
                                * 100.0) as f32
                        }
                        None => 0.0,
                    }
                } else {
                    0.0
                };

                let (gpu_memory, gpu_index) = match gpu_map.get(&pid) {
                    Some(&(m, i)) => (Some(m), Some(i)),
                    None => (None, None),
                };

                procs.push(SystemProcess {
                    pid,
                    gpu_memory,
                    gpu_index,
                    username: String::new(),
                    command: stat.comm.clone(),
                    cpu_usage,
                    memory_usage: stat.rss * page,
                    state: stat.state,
                });
            }
        }
    }

    sort_processes(&mut procs, app_state.sort_mode);
    procs.truncate(TOP_N);

    for proc in procs.iter_mut() {
        match Process::new(proc.pid) {
            Ok(process) => {
                if let Ok(uid) = process.uid() {
                    proc.username = username_for(uid, &mut app_state.uid_cache);
                }
                match process.cmdline() {
                    Ok(cmd) if !cmd.join(" ").is_empty() => proc.command = cmd.join(" "),
                    _ => proc.command = format!("[{}]", proc.command),
                }
            }
            Err(_) => proc.command = format!("[{}]", proc.command),
        }
    }

    app_state.cpu_stats = CpuStats {
        model,
        logical_cores,
        per_core_usage,
        aggregate_usage,
        frequency_mhz,
        load_avg,
        mem_total,
        mem_used,
        swap_total,
        swap_used,
    };
    // Skip the bootstrap sample (always 0% — no prior delta) so the history
    // graph doesn't fill with a flat zero line before real readings exist.
    let had_baseline = app_state.prev_cpu_total.is_some();

    app_state.prev_cpu_total = Some(cur_total);
    app_state.prev_cpu_per_core = cur_per_core;
    app_state.prev_proc_times = new_prev;
    app_state.processes = procs;

    if had_baseline {
        // Keep sub-percent precision; rounding to u64 made idle ~0.3% look like 0.
        app_state
            .cpu_usage_history
            .push(f64::from(aggregate_usage).clamp(0.0, 100.0));
        while app_state.cpu_usage_history.len() > HISTORY_CAP {
            app_state.cpu_usage_history.remove(0);
        }
    }

    Ok(())
}

/// Build an InfluxDB write for system-wide CPU/memory (used with `--cpu` + Influx).
pub fn system_metrics_write_query(cpu: &CpuStats) -> influxdb::WriteQuery {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_nanos();

    influxdb::WriteQuery::new(influxdb::Timestamp::Nanoseconds(ts), "system_metrics")
        .add_field("cpu_usage", cpu.aggregate_usage as f64)
        .add_field("cpu_freq", cpu.frequency_mhz)
        .add_field("mem_used", cpu.mem_used as i64)
        .add_field("mem_total", cpu.mem_total as i64)
        .add_field("swap_used", cpu.swap_used as i64)
        .add_field("swap_total", cpu.swap_total as i64)
        .add_field("load_1", cpu.load_avg.0 as f64)
}

fn kernel_cpu_sample(t: &procfs::CpuTime) -> KernelCpuSample {
    let idle = t.idle + t.iowait.unwrap_or(0);
    let total = t.user
        + t.nice
        + t.system
        + t.idle
        + t.iowait.unwrap_or(0)
        + t.irq.unwrap_or(0)
        + t.softirq.unwrap_or(0)
        + t.steal.unwrap_or(0);
    KernelCpuSample { total, idle }
}

/// Percentage busy between two cumulative samples.
fn pct(prev: &KernelCpuSample, cur: &KernelCpuSample) -> f32 {
    let dt = cur.total.saturating_sub(prev.total);
    let di = cur.idle.saturating_sub(prev.idle).min(dt);
    if dt == 0 {
        0.0
    } else {
        ((dt - di) as f64 / dt as f64 * 100.0) as f32
    }
}

fn read_cpu_model_freq() -> Option<(String, f64)> {
    let info = procfs::CpuInfo::current().ok()?;
    let model = info.model_name(0).unwrap_or("Unknown CPU").to_string();
    let mut sum = 0.0;
    let mut count = 0u32;
    for i in 0..info.num_cores() {
        if let Some(mhz) = info.get_field(i, "cpu MHz") {
            if let Ok(v) = mhz.trim().parse::<f64>() {
                sum += v;
                count += 1;
            }
        }
    }
    let freq = if count > 0 { sum / count as f64 } else { 0.0 };
    Some((model, freq))
}

/// Map PID -> (total_used_gpu_memory, gpu_index) from NVML-collected GPU process lists.
pub fn build_gpu_map(gpu_infos: &[GpuInfo]) -> HashMap<i32, (u64, usize)> {
    let mut acc: HashMap<i32, (u64, usize, u64)> = HashMap::new();
    for gpu in gpu_infos {
        for proc in &gpu.processes {
            let pid = proc.pid as i32;
            let mem = proc.used_gpu_memory;
            acc.entry(pid)
                .and_modify(|e| {
                    e.0 += mem;
                    if mem > e.2 {
                        e.1 = gpu.index;
                        e.2 = mem;
                    }
                })
                .or_insert((mem, gpu.index, mem));
        }
    }
    acc.into_iter()
        .map(|(pid, (total, idx, _))| (pid, (total, idx)))
        .collect()
}

fn username_for(uid: u32, cache: &mut HashMap<u32, String>) -> String {
    if let Some(name) = cache.get(&uid) {
        return name.clone();
    }
    let name = User::from_uid(Uid::from_raw(uid))
        .ok()
        .flatten()
        .map(|u| u.name)
        .unwrap_or_else(|| uid.to_string());
    cache.insert(uid, name.clone());
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuProcessInfo;

    fn proc(cpu: f32, gpu: Option<u64>) -> SystemProcess {
        SystemProcess {
            pid: 1,
            gpu_memory: gpu,
            gpu_index: gpu.map(|_| 0),
            username: String::new(),
            command: String::new(),
            cpu_usage: cpu,
            memory_usage: 0,
            state: 'R',
        }
    }

    #[test]
    fn test_pct_idle_only() {
        let prev = KernelCpuSample { total: 0, idle: 0 };
        let cur = KernelCpuSample {
            total: 100,
            idle: 100,
        };
        assert_eq!(pct(&prev, &cur), 0.0);
    }

    #[test]
    fn test_pct_fully_busy() {
        let prev = KernelCpuSample { total: 0, idle: 0 };
        let cur = KernelCpuSample {
            total: 100,
            idle: 0,
        };
        assert_eq!(pct(&prev, &cur), 100.0);
    }

    #[test]
    fn test_pct_half_busy() {
        let prev = KernelCpuSample {
            total: 200,
            idle: 100,
        };
        let cur = KernelCpuSample {
            total: 400,
            idle: 200,
        };
        assert_eq!(pct(&prev, &cur), 50.0);
    }

    #[test]
    fn test_pct_no_elapsed_time() {
        let prev = KernelCpuSample {
            total: 100,
            idle: 50,
        };
        let cur = KernelCpuSample {
            total: 100,
            idle: 50,
        };
        assert_eq!(pct(&prev, &cur), 0.0);
    }

    #[test]
    fn test_sort_by_cpu() {
        let mut procs = vec![proc(10.0, None), proc(90.0, None), proc(50.0, None)];
        sort_processes(&mut procs, SortMode::Cpu);
        assert_eq!(procs[0].cpu_usage, 90.0);
        assert_eq!(procs[1].cpu_usage, 50.0);
        assert_eq!(procs[2].cpu_usage, 10.0);
    }

    #[test]
    fn test_sort_by_gpu_memory_puts_gpu_procs_first() {
        let mut procs = vec![
            proc(99.0, None),
            proc(1.0, Some(2048)),
            proc(1.0, Some(8192)),
        ];
        sort_processes(&mut procs, SortMode::GpuMemory);
        assert_eq!(procs[0].gpu_memory, Some(8192));
        assert_eq!(procs[1].gpu_memory, Some(2048));
        assert_eq!(procs[2].gpu_memory, None);
    }

    #[test]
    fn test_sort_by_cpu_stable_on_ties() {
        let mut procs = vec![
            SystemProcess {
                pid: 30,
                gpu_memory: None,
                gpu_index: None,
                username: String::new(),
                command: String::new(),
                cpu_usage: 5.0,
                memory_usage: 0,
                state: 'R',
            },
            SystemProcess {
                pid: 10,
                gpu_memory: None,
                gpu_index: None,
                username: String::new(),
                command: String::new(),
                cpu_usage: 5.0,
                memory_usage: 0,
                state: 'R',
            },
            SystemProcess {
                pid: 20,
                gpu_memory: None,
                gpu_index: None,
                username: String::new(),
                command: String::new(),
                cpu_usage: 5.0,
                memory_usage: 0,
                state: 'R',
            },
        ];
        sort_processes(&mut procs, SortMode::Cpu);
        assert_eq!(
            procs.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn test_meter_bar_clamps() {
        let (filled, empty) = meter_bar(50.0);
        assert_eq!(filled.len() + empty.len(), BAR_WIDTH);
        let (filled, _) = meter_bar(150.0);
        assert_eq!(filled.len(), BAR_WIDTH);
        let (filled, empty) = meter_bar(0.0);
        assert_eq!(filled.len(), 0);
        assert_eq!(empty.len(), BAR_WIDTH);
    }

    #[test]
    fn test_build_gpu_map_sums_and_picks_dominant() {
        let gpus = vec![
            GpuInfo {
                index: 0,
                name: "A".into(),
                temperature: 0,
                utilization: 0,
                memory_used: 0,
                memory_total: 0,
                power_usage: 0,
                power_limit: 0,
                clock_freq: 0,
                processes: vec![GpuProcessInfo {
                    pid: 42,
                    used_gpu_memory: 100,
                    username: String::new(),
                    command: String::new(),
                    cpu_percent: None,
                    memory_usage: 0,
                }],
            },
            GpuInfo {
                index: 1,
                name: "B".into(),
                temperature: 0,
                utilization: 0,
                memory_used: 0,
                memory_total: 0,
                power_usage: 0,
                power_limit: 0,
                clock_freq: 0,
                processes: vec![GpuProcessInfo {
                    pid: 42,
                    used_gpu_memory: 500,
                    username: String::new(),
                    command: String::new(),
                    cpu_percent: None,
                    memory_usage: 0,
                }],
            },
        ];
        let map = build_gpu_map(&gpus);
        assert_eq!(map.get(&42), Some(&(600, 1)));
    }

    #[test]
    fn test_system_metrics_write_query_builds() {
        let stats = CpuStats {
            aggregate_usage: 12.5,
            frequency_mhz: 2400.0,
            mem_used: 1024,
            mem_total: 4096,
            swap_used: 0,
            swap_total: 0,
            load_avg: (0.5, 0.4, 0.3),
            ..Default::default()
        };
        let _q = system_metrics_write_query(&stats);
    }
}
