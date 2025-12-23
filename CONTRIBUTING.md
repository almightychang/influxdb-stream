# Contributing to influxdb-stream

Thank you for your interest in contributing to influxdb-stream! This document provides guidelines and instructions for contributing.

## Code of Conduct

This project adheres to the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). By participating, you are expected to uphold this code.

## Getting Started

### Prerequisites

- Rust 1.85 or later
- Docker and Docker Compose (for integration tests)

### Setup

```bash
git clone https://github.com/almightychang/influxdb-stream.git
cd influxdb-stream
cargo build
```

## How to Contribute

### Reporting Bugs

Before submitting a bug report, please search existing issues to avoid duplicates.

When filing a bug report, include:

- Rust version (`rustc --version`)
- Operating system and version
- Steps to reproduce
- Expected vs actual behavior
- Error messages or logs

### Suggesting Features

Feature requests are welcome! Please describe:

- The use case and motivation
- Proposed API (if applicable)
- Alternatives you've considered

### Pull Requests

1. Fork the repository and create your branch from `main`
2. Make your changes
3. Add or update tests as needed
4. Ensure all checks pass (see below)
5. Submit a pull request

## Development

### Running Tests

```bash
# Unit tests
cargo test --lib

# All tests (requires Docker)
./scripts/test-local.sh

# Or manually:
docker-compose up -d
cargo test
docker-compose down
```

### Code Style

We use standard Rust formatting and linting:

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy -- -D warnings
```

### Documentation

All public APIs must be documented. Build and verify docs with:

```bash
cargo doc --no-deps --open
```

### Benchmarks

```bash
./scripts/bench-local.sh
```

## Project Structure

```
src/
├── lib.rs      - Crate root and public exports
├── client.rs   - InfluxDB HTTP client
├── parser.rs   - Annotated CSV parser
├── types.rs    - FluxRecord, FluxColumn, FluxTableMetadata
├── value.rs    - Value enum for Flux data types
└── error.rs    - Error types
```

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
