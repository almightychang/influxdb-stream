#!/bin/bash
set -e

echo "=== influxdb-stream Benchmark Runner ==="
echo ""

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "Error: Docker is not running. Please start Docker first."
    exit 1
fi

# Check if InfluxDB is running
if ! curl -s http://localhost:8086/health | grep -q "pass"; then
    echo "InfluxDB is not running. Starting..."
    docker-compose up -d
    echo "Waiting for InfluxDB to be ready..."
    sleep 5

    for i in {1..30}; do
        if curl -s http://localhost:8086/health | grep -q "pass"; then
            echo "InfluxDB is ready!"
            break
        fi
        sleep 1
    done
fi

echo ""
echo "Running benchmarks..."
echo "This may take several minutes..."
echo ""

cargo bench

echo ""
echo "=== Benchmarks complete! ==="
echo ""
echo "Results are saved in: target/criterion/"
echo "Open target/criterion/report/index.html for detailed graphs"
