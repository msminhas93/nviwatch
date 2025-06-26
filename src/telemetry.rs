use influxdb::{Client, Timestamp, WriteQuery};
use crate::gpu::info::GpuInfo;
use std::error::Error;
use tokio::runtime::Runtime;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct InfluxDBConfig {
    pub url: String,
    pub org: String,
    pub bucket: String,
    pub token: String,
}

pub fn send_to_influxdb(config: &InfluxDBConfig, gpu_infos: &[GpuInfo]) -> Result<(), Box<dyn Error>> {
    let client = Client::new(&config.url, &config.bucket).with_token(&config.token);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos();

    let queries: Vec<WriteQuery> = gpu_infos.iter().map(|gpu| {
        WriteQuery::new(Timestamp::Nanoseconds(timestamp), "gpu_metrics")
            .add_tag("gpu_index", gpu.index.to_string())
            .add_tag("gpu_name", gpu.name.clone())
            .add_field("temperature", gpu.temperature as f64)
            .add_field("utilization", gpu.utilization as f64)
            .add_field("memory_used", gpu.memory_used as i64)
            .add_field("memory_total", gpu.memory_total as i64)
            .add_field("power_usage", gpu.power_usage as f64)
            .add_field("power_limit", gpu.power_limit as f64)
            .add_field("clock_freq", gpu.clock_freq as f64)
    }).collect();


    let runtime = Runtime::new()?;
    runtime.block_on(async {
        client.query(queries).await?;
        Ok(())
    })
}
