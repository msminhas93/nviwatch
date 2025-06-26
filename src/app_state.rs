use crate::gpu::info::GpuInfo;

pub struct AppState {
    pub selected_process: usize,
    pub selected_gpu_tab: usize,
    pub gpu_infos: Vec<GpuInfo>,
    pub error_message: Option<String>,
    pub power_history: Vec<Vec<u64>>,
    pub utilization_history: Vec<Vec<u64>>,
    pub use_tabbed_graphs: bool,
    pub use_bar_charts: bool,
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
            memory_used: 4 * 1024 * 1024 * 1024, // 4GB
            memory_total: 8 * 1024 * 1024 * 1024, // 8GB
            power_usage: 150,
            power_limit: 200,
            clock_freq: 1800,
            processes: vec![], // Empty processes for testing
        }
    }

    #[test]
    fn test_app_state_initialization() {
        let state = AppState {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: vec![],
            error_message: None,
            power_history: vec![],
            utilization_history: vec![],
            use_tabbed_graphs: true,
            use_bar_charts: false,
        };
        
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
        let state = AppState {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: vec![],
            error_message: None,
            power_history: vec![],
            utilization_history: vec![],
            use_tabbed_graphs: false,
            use_bar_charts: false,
        };
        
        let total_processes: usize = state.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum();
        assert_eq!(total_processes, 0);
    }

    #[test]
    fn test_total_processes_with_gpus() {
        let state = AppState {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: vec![
                create_test_gpu_info(0),
                create_test_gpu_info(1),
            ],
            error_message: None,
            power_history: vec![],
            utilization_history: vec![],
            use_tabbed_graphs: false,
            use_bar_charts: false,
        };
        
        let total_processes: usize = state.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum();
        assert_eq!(total_processes, 0); // No processes in test GPUs
    }

    #[test]
    fn test_can_select_process() {
        let state = AppState {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: vec![],
            error_message: None,
            power_history: vec![],
            utilization_history: vec![],
            use_tabbed_graphs: false,
            use_bar_charts: false,
        };
        // Should not be able to select any process when there are no GPUs
        let total_processes: usize = state.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum();
        assert!(!(0 < total_processes));
        assert!(!(1 < total_processes));
    }

    #[test]
    fn test_can_select_gpu_tab() {
        let state = AppState {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: vec![create_test_gpu_info(0)],
            error_message: None,
            power_history: vec![],
            utilization_history: vec![],
            use_tabbed_graphs: false,
            use_bar_charts: false,
        };
        
        // Should be able to select GPU 0, but not GPU 1
        assert!(0 < state.gpu_infos.len());
        assert!(1 >= state.gpu_infos.len());
    }

    #[test]
    fn test_error_message_handling() {
        let mut state = AppState {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: vec![],
            error_message: None,
            power_history: vec![],
            utilization_history: vec![],
            use_tabbed_graphs: false,
            use_bar_charts: false,
        };
        
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
