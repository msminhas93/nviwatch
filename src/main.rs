extern crate nvml_wrapper as nvml;
extern crate prettytable;
extern crate clap;
extern crate crossterm;

use nvml::Nvml;
use nvml::enum_wrappers::device::TemperatureSensor;
use prettytable::{Table, Row, Cell, format};
use clap::{Arg, Command};
use crossterm::{execute, terminal::{Clear, ClearType}};
use std::error::Error;
use std::io::stdout;
use std::thread::sleep;
use std::time::Duration;

// Define a struct to hold GPU information
struct GpuInfo {
    index: usize,
    name: String,
    temperature: u32,
    utilization: u32,
    memory_used: u64,
    memory_total: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command-line arguments
    let matches = Command::new("gpu-info-rs")
        .version("0.1.0")
        .author("Your Name <your.email@example.com>")
        .about("Displays GPU information in a tabular format")
        .arg(Arg::new("watch")
            .short('w')
            .long("watch")
            .value_name("MILLISECONDS")
            .help("Refresh interval in milliseconds")
            .required(false))
        .get_matches();

    // Get the watch interval if specified
    let watch_interval = matches.get_one::<String>("watch").map(|s| s.parse::<u64>().expect("Invalid number"));

    // Initialize NVML
    let nvml = Nvml::init()?;

    loop {
        // Get the number of available devices
        let device_count = nvml.device_count()?;

        // Create a vector to store GPU information
        let mut gpu_infos = Vec::new();

        // Loop through all devices
        for index in 0..device_count {
            let device = nvml.device_by_index(index)?;

            let name = device.name()?;
            let temperature = device.temperature(TemperatureSensor::Gpu)?;
            let utilization = device.utilization_rates()?.gpu;
            let memory = device.memory_info()?;

            gpu_infos.push(GpuInfo {
                index: index as usize, // Convert u32 to usize
                name,
                temperature,
                utilization,
                memory_used: memory.used / 1_048_576, // Convert bytes to MB
                memory_total: memory.total / 1_048_576, // Convert bytes to MB
            });
        }

        // Clear the terminal screen
        execute!(stdout(), Clear(ClearType::All))?;

        // Create a table
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

        // Add header row
        table.add_row(Row::new(vec![
            Cell::new("GPU").style_spec("Fb"),
            Cell::new("Name").style_spec("Fb"),
            Cell::new("Temp").style_spec("Fb"),
            Cell::new("Util").style_spec("Fb"),
            Cell::new("Memory").style_spec("Fb"),
        ]));

        // Add GPU information rows
        for gpu in gpu_infos {
            table.add_row(Row::new(vec![
                Cell::new(&gpu.index.to_string()).style_spec("Fg"),
                Cell::new(&gpu.name).style_spec("Fy"),
                Cell::new(&format!("{}°C", gpu.temperature)).style_spec("Fr"),
                Cell::new(&format!("{}%", gpu.utilization)).style_spec("Fc"),
                Cell::new(&format!("{}/{}MB", gpu.memory_used, gpu.memory_total)).style_spec("Fm"),
            ]));
        }

        // Print the table
        table.printstd();

        // Check if watch interval is specified
        if let Some(interval) = watch_interval {
            sleep(Duration::from_millis(interval));
        } else {
            break;
        }
    }

    Ok(())
}