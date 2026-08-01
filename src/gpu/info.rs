use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::Result;
use crate::app_state::AppState;
use crate::error::NviError;
use crate::gpu::GpuProcessInfo;
use crate::utils::system::get_process_info;
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

impl TryFrom<(usize, Device<'_>)> for GpuInfo {
    type Error = NviError;

    // TODO: Can probably streamline this a fair bit. Literally just moved the existing code over
    fn try_from(device_data: (usize, Device<'_>)) -> std::result::Result<Self, Self::Error> {
        let index = device_data.0;
        let device = device_data.1;

        let name = device.name()?;
        let temperature = device.temperature(TemperatureSensor::Gpu)?;
        let utilization = device.utilization_rates()?.gpu;
        let memory = device.memory_info()?;

        let power_usage = device.power_usage()? / 1000; // Convert mW to W
        let power_limit = device.enforced_power_limit()? / 1000; // Convert mW to W
        let clock_freq = device.clock_info(nvml::enum_wrappers::device::Clock::Graphics)?;

        let compute_processes: Vec<GpuProcessInfo> = device
            .running_compute_processes()?
            .into_iter()
            .filter_map(|p| {
                let used_gpu_memory = match p.used_gpu_memory {
                    nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes,
                    nvml::enums::device::UsedGpuMemory::Unavailable => 0,
                };
                get_process_info(p.pid, used_gpu_memory)
            })
            .collect();

        let graphics_processes: Vec<GpuProcessInfo> = device
            .running_graphics_processes()?
            .into_iter()
            .filter_map(|p| {
                let used_gpu_memory = match p.used_gpu_memory {
                    nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes,
                    nvml::enums::device::UsedGpuMemory::Unavailable => 0,
                };
                get_process_info(p.pid, used_gpu_memory)
            })
            .collect();

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
            processes: [compute_processes, graphics_processes].concat(),
        })
    }
}

impl From<&GpuInfo> for WriteQuery {
    fn from(gpu: &GpuInfo) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();

        // TODO: But can we optimize this? (for perf or just... make it simpler?)
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

const HISTORY_CAP: usize = 60;

// Called from AppState::update on each poll interval (not every UI frame).
pub fn collect_gpu_info(nvml: &Nvml, app_state: &mut AppState) -> Result<Vec<GpuInfo>> {
    let device_count = nvml.device_count()?;
    let mut gpu_infos = Vec::new();

    for index in 0..device_count as usize {
        let device = nvml.device_by_index(index as u32)?;

        let gpu_info = GpuInfo::try_from((index, device))?;

        // Update historical data
        if app_state.power_history.len() <= index {
            app_state.power_history.push(Vec::new());
            app_state.utilization_history.push(Vec::new());
        }

        // Add the current data point
        app_state.power_history[index].push(gpu_info.power_usage as u64);
        app_state.utilization_history[index].push(gpu_info.utilization as u64);

        // Keep only the last 60 data points (for a 1-minute graph at ~1s intervals)
        if app_state.power_history[index].len() > 60 {
            let excess = app_state.power_history[index].len() - 60;
            app_state.power_history[index].drain(..excess);
            app_state.utilization_history[index].drain(..excess);
        }

        gpu_infos.push(gpu_info);
    }

    Ok(gpu_infos)
}
