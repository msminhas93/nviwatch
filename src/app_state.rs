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
