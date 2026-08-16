use std::collections::HashMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::Result;
use crate::app_state::AppState;
use crate::gpu::GpuProcessInfo;
use crate::utils::system::{CpuSample, get_process_info};
use influxdb::WriteQuery;
use nvml::enum_wrappers::device::TemperatureSensor;
use nvml::{Device, Nvml};

// PERF: we can either optimize this based on the operations we're doing,
// or we can find a way to use a bitmask,
// or simply back it
pub struct GpuInfo {
    pub index: usize,
    pub name: String,
    pub temperature: u32,
    pub utilization: u32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub power_usage: u32,
    pub power_limit: u32,
    pub clock_freq: u32,
    pub processes: Vec<GpuProcessInfo>,
}

/// Map NVML process infos into `GpuProcessInfo`, resolving used GPU memory.
fn collect_gpu_processes(
    processes: impl IntoIterator<Item = nvml::struct_wrappers::device::ProcessInfo>,
    previous_samples: &HashMap<u32, CpuSample>,
    next_samples: &mut HashMap<u32, CpuSample>,
) -> Vec<GpuProcessInfo> {
    processes
        .into_iter()
        .filter_map(|p| {
            let used_gpu_memory = match p.used_gpu_memory {
                nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes,
                nvml::enums::device::UsedGpuMemory::Unavailable => 0,
            };
            let (info, sample) =
                get_process_info(p.pid, used_gpu_memory, previous_samples.get(&p.pid))?;
            next_samples.insert(p.pid, sample);
            Some(info)
        })
        .collect()
}

/// Lean PID + GPU-memory records for `--cpu` mode (system scan enriches later).
/// Compute and graphics lists can overlap; keep the larger memory figure per PID.
fn collect_gpu_processes_lean(
    processes: impl IntoIterator<Item = nvml::struct_wrappers::device::ProcessInfo>,
) -> Vec<GpuProcessInfo> {
    let mut out: Vec<GpuProcessInfo> = Vec::new();
    for p in processes {
        let used_gpu_memory = match p.used_gpu_memory {
            nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes,
            nvml::enums::device::UsedGpuMemory::Unavailable => 0,
        };
        if let Some(existing) = out.iter_mut().find(|e| e.pid == p.pid) {
            if used_gpu_memory > existing.used_gpu_memory {
                existing.used_gpu_memory = used_gpu_memory;
            }
        } else {
            out.push(GpuProcessInfo {
                pid: p.pid,
                used_gpu_memory,
                username: String::new(),
                command: String::new(),
                cpu_percent: None,
                memory_usage: 0,
            });
        }
    }
    out
}

fn gpu_info_from_device(
    index: usize,
    device: Device<'_>,
    previous_samples: &HashMap<u32, CpuSample>,
    next_samples: &mut HashMap<u32, CpuSample>,
    cpu_monitoring: bool,
) -> Result<GpuInfo> {
    let name = device.name()?;
    let temperature = device.temperature(TemperatureSensor::Gpu)?;
    let utilization = device.utilization_rates()?.gpu;
    let memory = device.memory_info()?;

    let power_usage = device.power_usage()? / 1000; // Convert mW to W
    let power_limit = device.enforced_power_limit()? / 1000; // Convert mW to W
    let clock_freq = device.clock_info(nvml::enum_wrappers::device::Clock::Graphics)?;

    let processes = if cpu_monitoring {
        collect_gpu_processes_lean(
            device
                .running_compute_processes()?
                .into_iter()
                .chain(device.running_graphics_processes()?),
        )
    } else {
        let compute_processes = collect_gpu_processes(
            device.running_compute_processes()?,
            previous_samples,
            next_samples,
        );
        let graphics_processes = collect_gpu_processes(
            device.running_graphics_processes()?,
            previous_samples,
            next_samples,
        );
        [compute_processes, graphics_processes].concat()
    };

    Ok(GpuInfo {
        index,
        name,
        temperature,
        utilization,
        memory_used: memory.used,
        memory_total: memory.total,
        power_usage,
        power_limit,
        clock_freq,
        processes,
    })
}

impl From<&GpuInfo> for WriteQuery {
    fn from(gpu: &GpuInfo) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();

        // clone required by WriteQuery tags (Type::Text owns String; API has no Cow path)
        WriteQuery::new(influxdb::Timestamp::Nanoseconds(ts), "gpu_metrics")
            .add_tag("gpu_index", gpu.index.to_string())
            .add_tag("gpu_name", gpu.name.clone())
            .add_field("temperature", gpu.temperature as f64)
            .add_field("utilization", gpu.utilization as f64)
            .add_field("memory_used", gpu.memory_used as i64)
            .add_field("memory_total", gpu.memory_total as i64)
            .add_field("power_usage", gpu.power_usage as f64)
            .add_field("power_limit", gpu.power_limit as f64)
            .add_field("clock_freq", gpu.clock_freq as f64)
    }
}

/// Max samples kept per GPU for the ~1-minute power/utilization graphs (~1s interval).
const HISTORY_CAP: usize = 60;

// Called from AppState::update on each poll interval (not every UI frame).
pub fn collect_gpu_info(nvml: &Nvml, app_state: &mut AppState) -> Result<Vec<GpuInfo>> {
    let device_count = nvml.device_count()? as usize;
    let mut gpu_infos = Vec::new();
    // Borrow prior samples by clone so a mid-poll NVML failure does not wipe
    // baselines (which would force CPU% back to "—" until the next successful window).
    let previous_samples = app_state.cpu_samples.clone();
    let mut next_samples = HashMap::new();

    let cpu_monitoring = app_state.cpu_monitoring;

    for index in 0..device_count {
        let device = nvml.device_by_index(index as u32)?;

        let gpu_info = gpu_info_from_device(
            index,
            device,
            &previous_samples,
            &mut next_samples,
            cpu_monitoring,
        )?;

        // Update historical data
        if app_state.power_history.len() <= index {
            app_state.power_history.push(Vec::new());
            app_state.utilization_history.push(Vec::new());
        }

        // Add the current data point
        app_state.power_history[index].push(gpu_info.power_usage as u64);
        app_state.utilization_history[index].push(gpu_info.utilization as u64);

        // Keep only the last HISTORY_CAP data points (1-minute graph at ~1s intervals)
        if app_state.power_history[index].len() > HISTORY_CAP {
            let excess = app_state.power_history[index].len() - HISTORY_CAP;
            app_state.power_history[index].drain(..excess);
            app_state.utilization_history[index].drain(..excess);
        }

        gpu_infos.push(gpu_info);
    }

    if device_count < app_state.power_history.len() {
        app_state.power_history.truncate(device_count);
        app_state.utilization_history.truncate(device_count);
    }

    // Per-process CPU samples only matter in GPU-only mode.
    if !cpu_monitoring {
        app_state.cpu_samples = next_samples;
    }

    Ok(gpu_infos)
}
