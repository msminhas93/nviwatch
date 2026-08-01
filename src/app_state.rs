use crate::gpu::info::GpuInfo;

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
    pub pending_g: bool,
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
            pending_g: false,
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
            pending_g: false,
        }
    }
}

impl AppState {
    pub fn should_update(&mut self, interval_ms: u64) -> bool {
        if self.last_update.elapsed() < std::time::Duration::from_millis(interval_ms) {
            false
        } else {
            self.last_update = std::time::Instant::now();
            true
        }
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
    }

    #[test]
    fn test_total_processes_empty() {
        let state = AppState::default();
        let total_processes: usize = state.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum();
        assert_eq!(total_processes, 0);
    }

    #[test]
    fn test_total_processes_with_gpus() {
        let mut state = AppState::default();
        state.gpu_infos.push(create_test_gpu_info(0));
        state.gpu_infos.push(create_test_gpu_info(1));

        let total_processes: usize = state.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum();
        assert_eq!(total_processes, 0); // No processes in test GPUs
    }

    #[test]
    fn test_can_select_process() {
        let state = AppState::default();
        // Should not be able to select any process when there are no GPUs
        let total_processes: usize = state.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum();
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
