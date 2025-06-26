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

impl InfluxDBConfig {
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.url.is_empty() {
            return Err("InfluxDB URL cannot be empty".into());
        }
        if self.org.is_empty() {
            return Err("InfluxDB organization cannot be empty".into());
        }
        if self.bucket.is_empty() {
            return Err("InfluxDB bucket cannot be empty".into());
        }
        if self.token.is_empty() {
            return Err("InfluxDB token cannot be empty".into());
        }
        Ok(())
    }
}

pub fn send_to_influxdb(config: &InfluxDBConfig, gpu_infos: &[GpuInfo]) -> Result<(), Box<dyn Error>> {
    // Validate configuration first
    config.validate()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::info::GpuInfo;

    fn create_test_gpu_info() -> GpuInfo {
        GpuInfo {
            index: 0,
            name: "Test GPU".to_string(),
            temperature: 75,
            utilization: 50,
            memory_used: 4 * 1024 * 1024 * 1024, // 4GB
            memory_total: 8 * 1024 * 1024 * 1024, // 8GB
            power_usage: 150,
            power_limit: 200,
            clock_freq: 1800,
            processes: vec![],
        }
    }

    #[test]
    fn test_influxdb_config_validation_valid() {
        let config = InfluxDBConfig {
            url: "http://localhost:8086".to_string(),
            org: "my-org".to_string(),
            bucket: "gpu-metrics".to_string(),
            token: "my-token".to_string(),
        };
        
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_influxdb_config_validation_empty_url() {
        let config = InfluxDBConfig {
            url: "".to_string(),
            org: "my-org".to_string(),
            bucket: "gpu-metrics".to_string(),
            token: "my-token".to_string(),
        };
        
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_influxdb_config_validation_empty_org() {
        let config = InfluxDBConfig {
            url: "http://localhost:8086".to_string(),
            org: "".to_string(),
            bucket: "gpu-metrics".to_string(),
            token: "my-token".to_string(),
        };
        
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_influxdb_config_validation_empty_bucket() {
        let config = InfluxDBConfig {
            url: "http://localhost:8086".to_string(),
            org: "my-org".to_string(),
            bucket: "".to_string(),
            token: "my-token".to_string(),
        };
        
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_influxdb_config_validation_empty_token() {
        let config = InfluxDBConfig {
            url: "http://localhost:8086".to_string(),
            org: "my-org".to_string(),
            bucket: "gpu-metrics".to_string(),
            token: "".to_string(),
        };
        
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_send_to_influxdb_with_empty_gpu_list() {
        let config = InfluxDBConfig {
            url: "http://localhost:8086".to_string(),
            org: "my-org".to_string(),
            bucket: "gpu-metrics".to_string(),
            token: "my-token".to_string(),
        };
        
        let gpu_infos: Vec<GpuInfo> = vec![];
        
        // This should fail because we can't connect to localhost:8086 in tests
        // but it should pass validation
        let result = send_to_influxdb(&config, &gpu_infos);
        // We expect this to fail due to connection, not validation
        assert!(result.is_err());
    }

    #[test]
    fn test_send_to_influxdb_with_invalid_config() {
        let config = InfluxDBConfig {
            url: "".to_string(), // Invalid empty URL
            org: "my-org".to_string(),
            bucket: "gpu-metrics".to_string(),
            token: "my-token".to_string(),
        };
        
        let gpu_infos = vec![create_test_gpu_info()];
        
        let result = send_to_influxdb(&config, &gpu_infos);
        assert!(result.is_err());
    }
} 