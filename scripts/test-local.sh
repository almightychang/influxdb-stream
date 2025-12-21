#!/bin/bash
set -e

echo "=== influxdb-stream Local Test Runner ==="
echo ""

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "Error: Docker is not running. Please start Docker first."
    exit 1
fi

# Start InfluxDB
echo "1. Starting InfluxDB..."
docker-compose up -d
echo "   Waiting for InfluxDB to be ready..."
sleep 5

# Wait for health check
for i in {1..30}; do
    if curl -s http://localhost:8086/health | grep -q "pass"; then
        echo "   InfluxDB is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "   Error: InfluxDB failed to start"
        docker-compose logs
        exit 1
    fi
    sleep 1
done

echo ""
echo "2. Running unit tests..."
cargo test --lib

echo ""
echo "3. Running integration tests..."
cargo test --test integration -- --nocapture

echo ""
echo "=== All tests passed! ==="
echo ""
echo "To run benchmarks: ./scripts/bench-local.sh"
echo "To stop InfluxDB:  docker-compose down"
