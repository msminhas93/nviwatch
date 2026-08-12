use crate::Result;
use crate::error::NviError;
use crate::gpu::GpuProcessInfo;
use crate::gpu::info::{GpuInfo, collect_gpu_info};
use crate::influx_local::{InfluxDBConfig, TempInfluxConfig};
use crate::keybinds::PendingOp;
use crate::system_monitor::{
    CpuStats, KernelCpuSample, SortMode, SystemProcess, collect_system_stats, rebuild_process_tray,
    system_metrics_write_query,
};
use crate::utils::system::CpuSample;
use nvml::Nvml;
use std::cmp::Reverse;
use std::collections::HashMap;

pub struct AppState {
    /// `None` until the first poll so startup draws data immediately, then
    /// spaces refreshes by `--watch` thereafter.
    last_update: Option<std::time::Instant>,
    /// Master switch for the CPU/system feature set (from `--cpu`).
    pub cpu_monitoring: bool,
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
    /// Used only in GPU-only mode.
    pub cpu_samples: HashMap<u32, CpuSample>,

    // ---- CPU / system side (`--cpu`) ----
    /// Last successful full process scan (pre top-N truncate). Sort toggles
    /// rebuild the tray from this so the new metric truly re-picks top N.
    pub process_scan: Vec<SystemProcess>,
    pub processes: Vec<SystemProcess>,
    pub sort_mode: SortMode,
    pub cpu_stats: CpuStats,
    /// Aggregate CPU% history for the line graph (float so sub-1% idle load is visible).
    pub cpu_usage_history: Vec<f64>,
    pub prev_cpu_total: Option<KernelCpuSample>,
    pub prev_cpu_per_core: Vec<KernelCpuSample>,
    pub prev_proc_times: HashMap<i32, u64>,
    pub uid_cache: HashMap<u32, String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            last_update: None,
            cpu_monitoring: false,
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
            process_scan: Vec::new(),
            processes: Vec::new(),
            sort_mode: SortMode::default(),
            cpu_stats: CpuStats::default(),
            cpu_usage_history: Vec::new(),
            prev_cpu_total: None,
            prev_cpu_per_core: Vec::new(),
            prev_proc_times: HashMap::new(),
            uid_cache: HashMap::new(),
        }
    }
}

impl From<&clap::ArgMatches> for AppState {
    fn from(matches: &clap::ArgMatches) -> Self {
        let use_tabbed_graphs = matches.get_flag("tabbed-graphs");
        let use_bar_charts = matches.get_flag("bar-chart");
        let cpu_monitoring = matches.get_flag("cpu");

        Self {
            last_update: None,
            cpu_monitoring,
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
            process_scan: Vec::new(),
            processes: Vec::new(),
            sort_mode: SortMode::default(),
            cpu_stats: CpuStats::default(),
            cpu_usage_history: Vec::new(),
            prev_cpu_total: None,
            prev_cpu_per_core: Vec::new(),
            prev_proc_times: HashMap::new(),
            uid_cache: HashMap::new(),
        }
    }
}

impl AppState {
    /// Rows in the active process tray (GPU-only or system-wide).
    pub fn total_process_count(&self) -> usize {
        if self.cpu_monitoring {
            self.processes.len()
        } else {
            self.gpu_infos.iter().map(|g| g.processes.len()).sum()
        }
    }

    /// Same order as the GPU-only process table: flatten then GPU-memory desc.
    ///
    /// Selection, kill, and render must all go through this (or
    /// `selected_process_entry` / `selected_kill_target`) so the index never
    /// points at different rows.
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

    /// Process under the current selection in GPU-only mode, in display order.
    pub fn selected_process_entry(&self) -> Option<(usize, &GpuProcessInfo)> {
        self.processes_display_order()
            .get(self.selected_process)
            .copied()
    }

    /// PID + command for the row currently highlighted (either tray).
    pub fn selected_kill_target(&self) -> Option<(u32, &str)> {
        if self.cpu_monitoring {
            self.processes
                .get(self.selected_process)
                .map(|p| (p.pid as u32, p.command.as_str()))
        } else {
            self.selected_process_entry()
                .map(|(_, p)| (p.pid, p.command.as_str()))
        }
    }

    /// Toggle CPU% ↔ GPU-memory sort and rebuild top-N from the last full scan.
    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Cpu => SortMode::GpuMemory,
            SortMode::GpuMemory => SortMode::Cpu,
        };
        rebuild_process_tray(self);
        self.selected_process = 0;
    }

    /// True on the first call (so the UI is never blank for a full `--watch`
    /// interval), then true again only after `interval_ms` since the last poll.
    pub fn should_update(&mut self, interval_ms: u64) -> bool {
        match self.last_update {
            None => {
                self.last_update = Some(std::time::Instant::now());
                true
            }
            Some(t) if t.elapsed() >= std::time::Duration::from_millis(interval_ms) => {
                self.last_update = Some(std::time::Instant::now());
                true
            }
            Some(_) => false,
        }
    }

    fn clamp_selection(&mut self) {
        let total = self.total_process_count();
        if total == 0 {
            self.selected_process = 0;
        } else if self.selected_process >= total {
            self.selected_process = total - 1;
        }
    }

    /// Poll GPU info (and optionally system stats) and optionally write to InfluxDB.
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

        if self.cpu_monitoring
            && let Err(e) = collect_system_stats(self)
        {
            self.error_message = Some(format!("System info error: {e}"));
        }

        self.clamp_selection();

        let temp = TempInfluxConfig::try_from(matches)?;
        // Incomplete flags: skip Influx (same as pre-AppState guard). All four
        // present but invalid (e.g. empty) must still surface in the UI.
        if !temp.flags_complete() {
            return Ok(());
        }

        let config = match InfluxDBConfig::try_from(&temp) {
            Ok(config) => config,
            Err(e) => {
                self.error_message = Some(e.to_string());
                return Ok(());
            }
        };

        let influx_client =
            influxdb::Client::new(&config.url, &config.bucket).with_token(&config.token);
        let mut queries: Vec<influxdb::WriteQuery> = self
            .gpu_infos
            .iter()
            .map(influxdb::WriteQuery::from)
            .collect();

        if self.cpu_monitoring {
            queries.push(system_metrics_write_query(&self.cpu_stats));
        }

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

    fn gpu_proc(pid: u32, mem: u64, command: &str) -> GpuProcessInfo {
        GpuProcessInfo {
            pid,
            used_gpu_memory: mem,
            username: "user".into(),
            command: command.into(),
            cpu_percent: Some(1.0),
            memory_usage: 0,
        }
    }

    #[test]
    fn test_app_state_initialization() {
        let state = AppState {
            use_tabbed_graphs: true,
            ..Default::default()
        };

        assert_eq!(state.selected_process, 0);
        assert_eq!(state.selected_gpu_tab, 0);
        assert!(state.gpu_infos.is_empty());
        assert!(state.error_message.is_none());
        assert!(state.power_history.is_empty());
        assert!(state.utilization_history.is_empty());
        assert!(state.use_tabbed_graphs);
        assert!(!state.use_bar_charts);
        assert_eq!(state.pending_op, PendingOp::None);
        assert!(!state.cpu_monitoring);
        assert!(state.processes.is_empty());
        assert_eq!(state.sort_mode, SortMode::Cpu);
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
    fn test_total_processes_cpu_mode_uses_system_list() {
        let mut state = AppState {
            cpu_monitoring: true,
            ..Default::default()
        };
        state.gpu_infos.push(GpuInfo {
            index: 0,
            name: "G".into(),
            temperature: 0,
            utilization: 0,
            memory_used: 0,
            memory_total: 0,
            power_usage: 0,
            power_limit: 0,
            clock_freq: 0,
            processes: vec![gpu_proc(1, 100, "gpu-only")],
        });
        state.processes.push(SystemProcess {
            pid: 99,
            gpu_memory: None,
            gpu_index: None,
            username: "u".into(),
            command: "sys".into(),
            cpu_usage: 10.0,
            memory_usage: 0,
            state: 'R',
        });
        assert_eq!(state.total_process_count(), 1);
    }

    #[test]
    fn test_selected_kill_target_gpu_mode() {
        let mut state = AppState::default();
        state.gpu_infos.push(GpuInfo {
            index: 0,
            name: "G".into(),
            temperature: 0,
            utilization: 0,
            memory_used: 0,
            memory_total: 0,
            power_usage: 0,
            power_limit: 0,
            clock_freq: 0,
            processes: vec![gpu_proc(10, 100, "small"), gpu_proc(20, 900, "big")],
        });
        // Display order is GPU-memory desc → big first.
        state.selected_process = 0;
        let (pid, cmd) = state.selected_kill_target().unwrap();
        assert_eq!(pid, 20);
        assert_eq!(cmd, "big");
    }

    #[test]
    fn test_selected_kill_target_cpu_mode() {
        let mut state = AppState {
            cpu_monitoring: true,
            ..Default::default()
        };
        state.processes.push(SystemProcess {
            pid: 7,
            gpu_memory: None,
            gpu_index: None,
            username: "u".into(),
            command: "top-proc".into(),
            cpu_usage: 50.0,
            memory_usage: 0,
            state: 'R',
        });
        let (pid, cmd) = state.selected_kill_target().unwrap();
        assert_eq!(pid, 7);
        assert_eq!(cmd, "top-proc");
    }

    #[test]
    fn test_cycle_sort_mode_rebuilds_top_n_from_full_scan() {
        let mut state = AppState::default();
        state.cpu_monitoring = true;
        state.selected_process = 3;
        // Full scan has a GPU-heavy process that would be truncated away under
        // CPU-sort top-2, but must surface after toggling to GPU-memory sort.
        state.process_scan = vec![
            SystemProcess {
                pid: 1,
                gpu_memory: None,
                gpu_index: None,
                username: String::new(),
                command: "cpu-a".into(),
                cpu_usage: 90.0,
                memory_usage: 0,
                state: 'R',
            },
            SystemProcess {
                pid: 2,
                gpu_memory: None,
                gpu_index: None,
                username: String::new(),
                command: "cpu-b".into(),
                cpu_usage: 80.0,
                memory_usage: 0,
                state: 'R',
            },
            SystemProcess {
                pid: 3,
                gpu_memory: Some(8192),
                gpu_index: Some(0),
                username: String::new(),
                command: "gpu-heavy".into(),
                cpu_usage: 1.0,
                memory_usage: 0,
                state: 'R',
            },
        ];
        rebuild_process_tray(&mut state);
        // With TOP_N=50 all three fit; still assert GPU sort promotes pid 3.
        assert_eq!(state.sort_mode, SortMode::Cpu);
        assert_eq!(state.processes[0].pid, 1);
        state.cycle_sort_mode();
        assert_eq!(state.sort_mode, SortMode::GpuMemory);
        assert_eq!(state.selected_process, 0);
        assert_eq!(state.processes[0].pid, 3);
    }

    #[test]
    fn test_can_select_process() {
        let state = AppState::default();
        // Should not be able to select any process when there are no GPUs
        let total_processes = state.total_process_count();
        assert!(total_processes == 0);
        assert!(total_processes <= 1);
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
    fn test_should_update_fires_immediately_then_respects_interval() {
        let mut state = AppState::default();
        // First poll must not wait for `--watch` — otherwise a 1500ms interval
        // leaves the TUI blank for 1.5s on startup.
        assert!(state.should_update(1500));
        assert!(!state.should_update(1500));
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
