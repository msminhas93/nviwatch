use std::fmt;

#[derive(Debug)]
pub enum NviError {
    Nvml(nvml::error::NvmlError),
    Influx(influxdb::Error),
    Io(std::io::Error),
    Process(String),
    Time(String),
    General(String),
}

impl std::error::Error for NviError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Nvml(e) => Some(e),
            Self::Influx(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Process(_) | Self::Time(_) | Self::General(_) => None,
        }
    }
}

impl fmt::Display for NviError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nvml(e) => write!(f, "nvml: {e}"),
            Self::Influx(e) => write!(f, "influxdb: {e}"),
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::Process(msg) => write!(f, "{msg}"),
            Self::Time(msg) => write!(f, "time error: {msg}"),
            Self::General(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<nvml::error::NvmlError> for NviError {
    fn from(e: nvml::error::NvmlError) -> Self {
        Self::Nvml(e)
    }
}

impl From<influxdb::Error> for NviError {
    fn from(e: influxdb::Error) -> Self {
        Self::Influx(e)
    }
}

impl From<std::io::Error> for NviError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<nix::errno::Errno> for NviError {
    fn from(e: nix::errno::Errno) -> Self {
        Self::Process(e.to_string())
    }
}

impl From<std::time::SystemTimeError> for NviError {
    fn from(e: std::time::SystemTimeError) -> Self {
        Self::Time(e.to_string())
    }
}
