use crate::gpu::process::GpuProcessInfo;
use crate::utils::system::get_process_info;
use crate::AppState;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use std::error::Error;

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
pub fn collect_gpu_info(
    nvml: &Nvml,
    app_state: &mut AppState,
) -> Result<Vec<GpuInfo>, Box<dyn Error>> {
    let device_count = nvml.device_count()?;
    let mut gpu_infos = Vec::new();

    for index in 0..device_count as usize {
        let device = nvml.device_by_index(index as u32)?;
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
        // Update historical data
        if app_state.power_history.len() <= index {
            app_state.power_history.push(Vec::new());
            app_state.utilization_history.push(Vec::new());
        }

        // Calculate how many seconds have passed since the last update
        let seconds_passed = if !app_state.power_history[index].is_empty() {
            (app_state.power_history[index].len() as u64).saturating_sub(60)
        } else {
            0
        };

        // Fill in missing data points with the last known value or 0
        for _ in 0..seconds_passed {
            let last_power = app_state.power_history[index].last().copied().unwrap_or(0);
            let last_util = app_state.utilization_history[index]
                .last()
                .copied()
                .unwrap_or(0);
            app_state.power_history[index].push(last_power);
            app_state.utilization_history[index].push(last_util);
        }

        // Add the current data point
        app_state.power_history[index].push(power_usage as u64);
        app_state.utilization_history[index].push(utilization as u64);

        // Keep only the last 60 data points (for a 1-minute graph)
        while app_state.power_history[index].len() > 60 {
            app_state.power_history[index].remove(0);
            app_state.utilization_history[index].remove(0);
        }

        gpu_infos.push(GpuInfo {
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
        });
    }

    Ok(gpu_infos)
}
