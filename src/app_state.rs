use crate::Result;
use crate::error::NviError;
use crate::gpu::GpuProcessInfo;
use crate::gpu::info::{GpuInfo, collect_gpu_info};
use crate::influx_local::{InfluxDBConfig, TempInfluxConfig};
use crate::keybinds::PendingOp;
use crate::utils::system::CpuSample;
use nvml::Nvml;
use std::cmp::Reverse;
use std::collections::HashMap;

pub struct AppState {
    last_update: std::time::Instant,
    pub selected_process: usize,
    pub selected_gpu_tab: usize,
    pub gpu_infos: Vec<GpuInfo>,
    pub error_message: Option<String>,
    pub power_history: Vec<Vec<u64>>,
    pub utilization_history: Vec<Vec<u64>>,
    pub use_tabbed_graphs: bool,
    pub use_bar_charts: bool,
    /// Multi-key chord in progress (`gg` / `dd`).
    pub pending_op: PendingOp,
    /// Full keymap overlay (`?`).
    pub show_help: bool,
    /// Vertical scroll offset (lines) while help is open.
    pub help_scroll: u16,
    /// Prior CPU tick samples for top-style %CPU deltas (keyed by PID).
    pub cpu_samples: HashMap<u32, CpuSample>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            last_update: std::time::Instant::now(),
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: Vec::new(),
            error_message: None,
            power_history: Vec::new(),
            utilization_history: Vec::new(),
            use_tabbed_graphs: false,
            use_bar_charts: false,
            pending_op: PendingOp::None,
            show_help: false,
            help_scroll: 0,
            cpu_samples: HashMap::new(),
        }
    }
}

impl From<&clap::ArgMatches> for AppState {
    fn from(matches: &clap::ArgMatches) -> Self {
        let use_tabbed_graphs = matches.get_flag("tabbed-graphs");
        let use_bar_charts = matches.get_flag("bar-chart");

        Self {
            last_update: std::time::Instant::now(),
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: Vec::new(),
            error_message: None,
            power_history: Vec::new(),
            utilization_history: Vec::new(),
            use_tabbed_graphs,
            use_bar_charts,
            pending_op: PendingOp::None,
            show_help: false,
            help_scroll: 0,
            cpu_samples: HashMap::new(),
        }
    }
}

impl AppState {
    /// Total GPU process rows (all devices). Count only — no sort.
    pub fn total_process_count(&self) -> usize {
        self.gpu_infos.iter().map(|g| g.processes.len()).sum()
    }

    /// Same order as the process table: flatten then GPU-memory desc.
    ///
    /// Selection, kill, and render must all go through this (or
    /// `selected_process_entry`) so the index never points at different rows.
    pub fn processes_display_order(&self) -> Vec<(usize, &GpuProcessInfo)> {
        let mut procs: Vec<_> = self
            .gpu_infos
            .iter()
            .enumerate()
            .flat_map(|(gpu_index, gpu)| {
                gpu.processes
                    .iter()
                    .map(move |process| (gpu_index, process))
            })
            .collect();
        procs.sort_by_key(|(_, p)| Reverse(p.used_gpu_memory));
        procs
    }

    /// Process under the current selection, in display order.
    pub fn selected_process_entry(&self) -> Option<(usize, &GpuProcessInfo)> {
        self.processes_display_order()
            .get(self.selected_process)
            .copied()
    }

    pub fn should_update(&mut self, interval_ms: u64) -> bool {
        if self.last_update.elapsed() < std::time::Duration::from_millis(interval_ms) {
            false
        } else {
            self.last_update = std::time::Instant::now();
            true
        }
    }

    /// Poll GPU info and optionally write metrics to InfluxDB.
    ///
    /// Influx is skipped unless all four `--influx-*` flags are present (same
    /// as the original main-loop guard). Partial/missing config is not an error.
    pub fn update(
        &mut self,
        nvml: &Nvml,
        matches: &clap::ArgMatches,
        runtime: &tokio::runtime::Runtime,
    ) -> Result<()> {
        self.gpu_infos = collect_gpu_info(nvml, self)?;

        let temp = TempInfluxConfig::try_from(matches)?;
        let Ok(config) = InfluxDBConfig::try_from(&temp) else {
            // No / incomplete influx flags: GPU poll still succeeded.
            return Ok(());
        };

        let influx_client =
            influxdb::Client::new(&config.url, &config.bucket).with_token(&config.token);
        let queries: Vec<influxdb::WriteQuery> = self
            .gpu_infos
            .iter()
            .map(influxdb::WriteQuery::from)
            .collect();

        runtime
            .block_on(async {
                influx_client
                    .query(queries)
                    .await
                    .map_err(|e| NviError::General(format!("InfluxDB Error: {}", e)))
            })
            .inspect_err(|e| {
                self.error_message = Some(e.to_string());
            })
            .ok();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::info::GpuInfo;

    fn create_test_gpu_info(index: u32) -> GpuInfo {
        GpuInfo {
            index: index as usize,
            name: format!("Test GPU {}", index),
            temperature: 75,
            utilization: 50,
            memory_used: 4 * 1024 * 1024 * 1024,  // 4GB
            memory_total: 8 * 1024 * 1024 * 1024, // 8GB
            power_usage: 150,
            power_limit: 200,
            clock_freq: 1800,
            processes: vec![], // Empty processes for testing
        }
    }

    #[test]
    fn test_app_state_initialization() {
        let mut state = AppState::default();
        state.use_tabbed_graphs = true;

        assert_eq!(state.selected_process, 0);
        assert_eq!(state.selected_gpu_tab, 0);
        assert!(state.gpu_infos.is_empty());
        assert!(state.error_message.is_none());
        assert!(state.power_history.is_empty());
        assert!(state.utilization_history.is_empty());
        assert!(state.use_tabbed_graphs);
        assert!(!state.use_bar_charts);
        assert_eq!(state.pending_op, PendingOp::None);
    }

    #[test]
    fn test_total_processes_empty() {
        let state = AppState::default();
        assert_eq!(state.total_process_count(), 0);
    }

    #[test]
    fn test_total_processes_with_gpus() {
        let mut state = AppState::default();
        state.gpu_infos.push(create_test_gpu_info(0));
        state.gpu_infos.push(create_test_gpu_info(1));

        assert_eq!(state.total_process_count(), 0); // No processes in test GPUs
    }

    #[test]
    fn test_can_select_process() {
        let state = AppState::default();
        // Should not be able to select any process when there are no GPUs
        let total_processes = state.total_process_count();
        assert!(!(0 < total_processes));
        assert!(!(1 < total_processes));
    }

    #[test]
    fn test_can_select_gpu_tab() {
        let mut state = AppState::default();
        state.gpu_infos.push(create_test_gpu_info(0));

        // Should be able to select GPU 0, but not GPU 1
        assert!(!state.gpu_infos.is_empty());
        assert!(1 >= state.gpu_infos.len());
    }

    #[test]
    fn test_error_message_handling() {
        let mut state = AppState::default();

        // Initially no error
        assert!(state.error_message.is_none());

        // Set an error
        state.error_message = Some("Test error message".to_string());
        assert!(state.error_message.is_some());
        assert_eq!(state.error_message.as_ref().unwrap(), "Test error message");

        // Clear the error
        state.error_message = None;
        assert!(state.error_message.is_none());
    }
}
