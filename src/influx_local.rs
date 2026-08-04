use crate::error::NviError;

#[derive(Default, Debug)]
pub struct TempInfluxConfig {
    url: Option<String>,
    org: Option<String>,
    bucket: Option<String>,
    token: Option<String>,
}

impl TempInfluxConfig {
    /// True when all four `--influx-*` flags were provided (values may still be empty).
    pub fn flags_complete(&self) -> bool {
        self.url.is_some()
            && self.org.is_some()
            && self.bucket.is_some()
            && self.token.is_some()
    }
}

impl TryFrom<&clap::ArgMatches> for TempInfluxConfig {
    type Error = NviError;

    fn try_from(matches: &clap::ArgMatches) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            url: matches.get_one::<String>("influx-url").cloned(),
            org: matches.get_one::<String>("influx-org").cloned(),
            bucket: matches.get_one::<String>("influx-bucket").cloned(),
            token: matches.get_one::<String>("influx-token").cloned(),
        })
    }
}

pub struct InfluxDBConfig {
    pub url: String,
    #[allow(dead_code)] // Currently we don't actually use `org` anywhere
    pub org: String,
    pub bucket: String,
    pub token: String,
}

impl TryFrom<&clap::ArgMatches> for InfluxDBConfig {
    type Error = NviError;

    fn try_from(matches: &clap::ArgMatches) -> std::result::Result<Self, Self::Error> {
        Self::try_from(&TempInfluxConfig::try_from(matches)?)
    }
}

impl TryFrom<&TempInfluxConfig> for InfluxDBConfig {
    type Error = NviError;

    fn try_from(temp: &TempInfluxConfig) -> std::result::Result<Self, Self::Error> {
        let attrs = [
            ("url", &temp.url),
            ("org", &temp.org),
            ("bucket", &temp.bucket),
            ("token", &temp.token),
        ];

        attrs.iter().try_for_each(|(name, value)| {
            if value.is_none() || value.as_ref().unwrap().is_empty() {
                Err(NviError::General(format!(
                    "InfluxDB {} cannot be empty",
                    name
                )))
            } else {
                Ok(())
            }
        })?;

        let url = temp.url.as_ref().unwrap().to_string();
        let org = temp.org.as_ref().unwrap().to_string();
        let bucket = temp.bucket.as_ref().unwrap().to_string();
        let token = temp.token.as_ref().unwrap().to_string();

        Ok(Self {
            url,
            org,
            bucket,
            token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::info::GpuInfo;
    use influxdb::WriteQuery;

    fn create_test_gpu_info() -> GpuInfo {
        GpuInfo {
            index: 0,
            name: "Test GPU".to_string(),
            temperature: 75,
            utilization: 50,
            memory_used: 4 * 1024 * 1024 * 1024,  // 4GB
            memory_total: 8 * 1024 * 1024 * 1024, // 8GB
            power_usage: 150,
            power_limit: 200,
            clock_freq: 1800,
            processes: vec![],
        }
    }

    #[test]
    fn test_influxdb_config_validation_valid() {
        let temp_config = TempInfluxConfig {
            url: Some("http://localhost:8086".to_string()),
            org: Some("my-org".to_string()),
            bucket: Some("gpu-metrics".to_string()),
            token: Some("my-token".to_string()),
        };

        let config = InfluxDBConfig::try_from(&temp_config);

        assert!(config.is_ok());
    }

    #[test]
    fn test_influxdb_config_validation_empty_url() {
        let config = TempInfluxConfig {
            url: Some("".to_string()),
            org: Some("my-org".to_string()),
            bucket: Some("gpu-metrics".to_string()),
            token: Some("my-token".to_string()),
        };

        let config = InfluxDBConfig::try_from(&config);

        assert!(config.is_err());
    }

    #[test]
    fn test_influxdb_config_validation_empty_org() {
        let config = TempInfluxConfig {
            url: Some("http://localhost:8086".to_string()),
            org: Some("".to_string()),
            bucket: Some("gpu-metrics".to_string()),
            token: Some("my-token".to_string()),
        };

        let config = InfluxDBConfig::try_from(&config);

        assert!(config.is_err());
    }

    #[test]
    fn test_influxdb_config_validation_empty_bucket() {
        let config = TempInfluxConfig {
            url: Some("http://localhost:8086".to_string()),
            org: Some("my-org".to_string()),
            bucket: Some("".to_string()),
            token: Some("my-token".to_string()),
        };

        let config = InfluxDBConfig::try_from(&config);
        assert!(config.is_err());
    }

    #[test]
    fn test_influxdb_config_validation_empty_token() {
        let config = TempInfluxConfig {
            url: Some("http://localhost:8086".to_string()),
            org: Some("my-org".to_string()),
            bucket: Some("gpu-metrics".to_string()),
            token: Some("".to_string()),
        };

        let config = InfluxDBConfig::try_from(&config);

        assert!(config.is_err());
    }

    #[test]
    fn test_gpu_info_to_write_query() {
        let gpu = create_test_gpu_info();
        let _query = WriteQuery::from(&gpu);
        // Conversion succeeds for a well-formed GpuInfo
    }
}
