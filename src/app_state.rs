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

impl AppState {
    pub fn new(use_tabbed_graphs: bool, use_bar_charts: bool) -> Self {
        Self {
            selected_process: 0,
            selected_gpu_tab: 0,
            gpu_infos: Vec::new(),
            error_message: None,
            power_history: Vec::new(),
            utilization_history: Vec::new(),
            use_tabbed_graphs,
            use_bar_charts,
        }
    }

    pub fn total_processes(&self) -> usize {
        self.gpu_infos.iter().map(|gpu| gpu.processes.len()).sum()
    }

    pub fn can_select_process(&self, index: usize) -> bool {
        index < self.total_processes()
    }

    pub fn can_select_gpu_tab(&self, index: usize) -> bool {
        index < self.gpu_infos.len()
    }

    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn has_error(&self) -> bool {
        self.error_message.is_some()
    }

    pub fn get_error_message(&self) -> Option<&String> {
        self.error_message.as_ref()
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
            memory_used: 4 * 1024 * 1024 * 1024, // 4GB
            memory_total: 8 * 1024 * 1024 * 1024, // 8GB
            power_usage: 150,
            power_limit: 200,
            clock_freq: 1800,
            processes: vec![], // Empty processes for testing
        }
    }

    #[test]
    fn test_app_state_new() {
        let state = AppState::new(true, false);
        
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
        let state = AppState::new(false, false);
        assert_eq!(state.total_processes(), 0);
    }

    #[test]
    fn test_total_processes_with_gpus() {
        let mut state = AppState::new(false, false);
        state.gpu_infos = vec![
            create_test_gpu_info(0),
            create_test_gpu_info(1),
        ];
        
        assert_eq!(state.total_processes(), 0); // No processes in test GPUs
    }

    #[test]
    fn test_can_select_process() {
        let state = AppState::new(false, false);
        
        // Should not be able to select any process when there are no GPUs
        assert!(!state.can_select_process(0));
        assert!(!state.can_select_process(1));
    }

    #[test]
    fn test_can_select_gpu_tab() {
        let mut state = AppState::new(false, false);
        
        // Should not be able to select any GPU tab when there are no GPUs
        assert!(!state.can_select_gpu_tab(0));
        assert!(!state.can_select_gpu_tab(1));
        
        // Add a GPU
        state.gpu_infos = vec![create_test_gpu_info(0)];
        
        // Should be able to select GPU 0, but not GPU 1
        assert!(state.can_select_gpu_tab(0));
        assert!(!state.can_select_gpu_tab(1));
    }

    #[test]
    fn test_error_handling() {
        let mut state = AppState::new(false, false);
        
        // Initially no error
        assert!(!state.has_error());
        assert!(state.get_error_message().is_none());
        
        // Set an error
        state.set_error("Test error message".to_string());
        assert!(state.has_error());
        assert_eq!(state.get_error_message().unwrap(), "Test error message");
        
        // Clear the error
        state.clear_error();
        assert!(!state.has_error());
        assert!(state.get_error_message().is_none());
    }

    #[test]
    fn test_error_message_content() {
        let mut state = AppState::new(false, false);
        
        let error_msg = "GPU temperature too high".to_string();
        state.set_error(error_msg.clone());
        
        assert_eq!(state.get_error_message().unwrap(), &error_msg);
    }
}
