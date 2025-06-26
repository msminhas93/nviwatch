#!/bin/bash
# This script automates the installation and setup of InfluxDB and a dashboard.

# --- Configuration --- 
# IMPORTANT: Change these values to your desired settings.
INFLUX_USERNAME="admin"
INFLUX_PASSWORD="password12345"
INFLUX_ORG="my-org"
INFLUX_BUCKET="gpu-metrics"
INFLUX_RETENTION="7d" # Data retention period
DASHBOARD_TEMPLATE_FILE="../config/gpu_dashboard_template.json"

# Exit immediately if a command exits with a non-zero status.
set -e

echo "--- Checking InfluxDB Status ---"

# Check if InfluxDB is already installed and configured
if command -v influx >/dev/null 2>&1; then
    echo "InfluxDB CLI is installed."
    
    # Check if InfluxDB service is running
    if systemctl is-active --quiet influxdb 2>/dev/null || service influxdb status >/dev/null 2>&1; then
        echo "InfluxDB service is running."
        
        # Check if we can connect and if the organization exists
        if influx ping >/dev/null 2>&1 && influx org list | grep -q "${INFLUX_ORG}" 2>/dev/null; then
            echo "InfluxDB is already configured with organization '${INFLUX_ORG}'."
            echo "Checking if dashboard already exists..."
            
            # Check if the dashboard already exists
            if influx dashboards | grep -q "GPU metrics" 2>/dev/null; then
                echo "Dashboard 'GPU metrics' already exists."
                echo
                echo "=== SETUP SKIPPED ==="
                echo "InfluxDB is already fully configured and ready to use."
                echo
                echo "To get your admin token, run:"
                echo "influx auth list"
                echo
                echo "Access the InfluxDB dashboard at: http://localhost:8086"
                echo "Username: ${INFLUX_USERNAME}"
                echo "Password: ${INFLUX_PASSWORD}"
                exit 0
            else
                echo "Dashboard not found. Will import dashboard template."
            fi
        else
            echo "InfluxDB is installed but not configured. Proceeding with setup..."
        fi
    else
        echo "InfluxDB service is not running. Proceeding with installation..."
    fi
else
    echo "InfluxDB CLI not found. Proceeding with installation..."
fi

echo "--- Starting InfluxDB Installation ---"

# 1. Add InfluxDB repository
wget -q https://repos.influxdata.com/influxdata-archive_compat.key
echo '393e8779c89ac8d958f81f942f9ad7fb82a25e133faddaf92e15b16e6ac9ce4c influxdata-archive_compat.key' | sha256sum -c
cat influxdata-archive_compat.key | gpg --dearmor | sudo tee /etc/apt/trusted.gpg.d/influxdata-archive_compat.gpg > /dev/null
echo 'deb [signed-by=/etc/apt/trusted.gpg.d/influxdata-archive_compat.gpg] https://repos.influxdata.com/debian stable main' | sudo tee /etc/apt/sources.list.d/influxdb.list
rm -f influxdata-archive_compat.key

# 2. Install InfluxDB 3 (with fallback to InfluxDB 2 if 3 is not available)
sudo apt-get update

# Try to install InfluxDB 3 first
if sudo apt-get install -y influxdb3-core 2>/dev/null; then
    echo "Successfully installed InfluxDB 3"
    INFLUXDB_VERSION="3"
else
    echo "InfluxDB 3 not available, falling back to InfluxDB 2"
    sudo apt-get install -y influxdb2
    INFLUXDB_VERSION="2"
fi

echo "--- Starting InfluxDB Service ---"

# 3. Start the service
sudo service influxdb start

# Wait a few seconds for the service to be ready
sleep 5

echo "--- Configuring InfluxDB ---"

# 4. Check if InfluxDB is already configured
if influx ping >/dev/null 2>&1 && influx org list | grep -q "${INFLUX_ORG}" 2>/dev/null; then
    echo "InfluxDB is already configured with organization '${INFLUX_ORG}'. Skipping setup."
    echo "Using existing configuration."
else
    echo "Setting up InfluxDB for the first time..."
    # Run non-interactive setup (compatible with both v2 and v3)
    influx setup --username "${INFLUX_USERNAME}" --password "${INFLUX_PASSWORD}" --org "${INFLUX_ORG}" --bucket "${INFLUX_BUCKET}" --retention "${INFLUX_RETENTION}" --force
fi

# 5. Import the dashboard template
echo "--- Importing Dashboard Template ---"

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DASHBOARD_TEMPLATE_PATH="${SCRIPT_DIR}/${DASHBOARD_TEMPLATE_FILE}"

if [ -f "${DASHBOARD_TEMPLATE_PATH}" ]; then
    # Apply the dashboard template (ignore errors if dashboard already exists)
    if influx apply --file "${DASHBOARD_TEMPLATE_PATH}" --org "${INFLUX_ORG}" --force yes 2>/dev/null; then
        echo "Dashboard imported successfully."
    else
        echo "Dashboard already exists or import failed. Continuing..."
    fi
else
    echo "WARNING: Dashboard template ${DASHBOARD_TEMPLATE_PATH} not found. Skipping import."
fi

echo "--- Setup Complete! ---"
echo "Your InfluxDB ${INFLUXDB_VERSION} and dashboard are ready to use."
echo
echo "To get your admin token, run the following command:"
echo "influx auth list"
echo
echo "Access the InfluxDB dashboard at: http://localhost:8086"
echo "Username: ${INFLUX_USERNAME}"
echo "Password: ${INFLUX_PASSWORD}"
