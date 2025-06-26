#!/bin/bash
# This script automates the installation and setup of InfluxDB and a dashboard.

# --- Configuration --- 
# IMPORTANT: Change these values to your desired settings.
INFLUX_USERNAME="admin"
INFLUX_PASSWORD="password12345"
INFLUX_ORG="my-org"
INFLUX_BUCKET="gpu-metrics"
INFLUX_RETENTION="7d" # Data retention period
DASHBOARD_TEMPLATE_FILE="gpu_dashboard_template.json"

# Exit immediately if a command exits with a non-zero status.
set -e

echo "--- Starting InfluxDB Installation ---"

# 1. Add InfluxDB repository
wget -q https://repos.influxdata.com/influxdata-archive_compat.key
echo '393e8779c89ac8d958f81f942f9ad7fb82a25e133faddaf92e15b16e6ac9ce4c influxdata-archive_compat.key' | sha256sum -c
cat influxdata-archive_compat.key | gpg --dearmor | sudo tee /etc/apt/trusted.gpg.d/influxdata-archive_compat.gpg > /dev/null
echo 'deb [signed-by=/etc/apt/trusted.gpg.d/influxdata-archive_compat.gpg] https://repos.influxdata.com/debian stable main' | sudo tee /etc/apt/sources.list.d/influxdb.list
rm -f influxdata-archive_compat.key

# 2. Install InfluxDB
sudo apt-get update
sudo apt-get install -y influxdb2

echo "--- Starting InfluxDB Service ---"

# 3. Start the service
sudo service influxdb start

# Wait a few seconds for the service to be ready
sleep 5

echo "--- Configuring InfluxDB ---"

# 4. Run non-interactive setup
influx setup --username "${INFLUX_USERNAME}" --password "${INFLUX_PASSWORD}" --org "${INFLUX_ORG}" --bucket "${INFLUX_BUCKET}" --retention "${INFLUX_RETENTION}" --force

# 5. Move and import the dashboard template
WINDOWS_USER_PATH="/home/msm/code/nviwatch"

echo "--- Importing Dashboard Template ---"
if [ -f "${WINDOWS_USER_PATH}/${DASHBOARD_TEMPLATE_FILE}" ]; then
    # Move the template from Windows to WSL
    mv "${WINDOWS_USER_PATH}/${DASHBOARD_TEMPLATE_FILE}" "/home/msm/${DASHBOARD_TEMPLATE_FILE}"
    
    # Apply the dashboard template
    influx apply --file "/home/msm/${DASHBOARD_TEMPLATE_FILE}" --org "${INFLUX_ORG}" --force
    echo "Dashboard imported successfully."
else
    echo "WARNING: Dashboard template ${WINDOWS_USER_PATH}/${DASHBOARD_TEMPLATE_FILE} not found. Skipping import."
fi

echo "--- Setup Complete! ---"
echo "Your InfluxDB and dashboard are ready to use."
echo
echo "To get your admin token, run the following command:"
echo "influx auth list"
