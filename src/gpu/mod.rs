pub mod info;

#[derive(Clone)]
pub struct GpuProcessInfo {
    pub pid: u32,
    pub used_gpu_memory: u64,
    pub username: String,
    pub command: String,
    /// Instant %CPU since the previous refresh (top-style). `None` on the first sample.
    pub cpu_percent: Option<f32>,
    pub memory_usage: u64,
}
