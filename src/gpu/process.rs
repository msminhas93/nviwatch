#[derive(Clone)]
pub struct GpuProcessInfo {
    pub pid: u32,
    pub used_gpu_memory: u64,
    pub username: String,
    pub command: String,
    pub cpu_usage: f32,
    pub memory_usage: u64,
}
