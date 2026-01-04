# Cloud Billing Simulator

A simulator for computing cloud object storage costs from API operations.

## Overview

Cloud Billing Simulator processes a log of storage operations and calculates the associated costs based on configurable pricing rules. It supports multiple cloud providers and can be used for:

- **Cost estimation**: Predict costs before running workloads
- **Cost analysis**: Analyze historical operation logs to understand spending
- **Provider comparison**: Compare costs across different cloud providers
- **Capacity planning**: Model costs for different workload patterns

## Supported Providers

The simulator includes pricing rules for:

- **AWS S3** - Full support including all storage classes (Standard, Intelligent-Tiering, Standard-IA, One Zone-IA, Glacier tiers)
- **Backblaze B2** - Including free egress multiplier
- **Cloudflare R2** - Including zero egress pricing
- **Azure Blob Storage** - Hot, Cool, Cold, and Archive tiers

## Installation

### From Source

```bash
git clone https://github.com/geoffw/cloud-billing-sim
cd cloud-billing-sim
cargo build --release
```

The binary will be at `target/release/cloud-billing-sim`.

### Optional Features

Enable cloud validation features to verify pricing rules against live APIs:

```bash
# AWS S3 validation
cargo build --release --features s3-validation

# Backblaze B2 validation
cargo build --release --features b2-validation

# Cloudflare R2 validation
cargo build --release --features r2-validation

# Azure Blob Storage validation
cargo build --release --features azure-validation

# All validation features
cargo build --release --features s3-validation,b2-validation,r2-validation,azure-validation
```

## Usage

### Basic Simulation

```bash
cloud-billing-sim simulate \
  --rules examples/aws-s3-us-east-1.toml \
  --operations examples/sample-workload.json
```

### Output Formats

```bash
# Text output (default)
cloud-billing-sim simulate -r rules.toml -o ops.json

# JSON output
cloud-billing-sim simulate -r rules.toml -o ops.json --format json

# Per-object cost breakdown
cloud-billing-sim simulate -r rules.toml -o ops.json --per-object
```

### Validate Files

```bash
# Validate pricing rules
cloud-billing-sim validate-rules --rules examples/aws-s3-us-east-1.toml

# Validate operations file
cloud-billing-sim validate-operations --operations examples/sample-workload.json
```

### Verbose Output

```bash
# Info level logging
cloud-billing-sim -v simulate -r rules.toml -o ops.json

# Debug level logging
cloud-billing-sim -vv simulate -r rules.toml -o ops.json
```

## Supported Operations

The simulator supports the following storage operations:

| Operation | Description |
|-----------|-------------|
| `put_object` | Upload or create an object |
| `get_object` | Download or retrieve an object |
| `delete_object` | Delete an object |
| `copy_object` | Copy an object within or across buckets |
| `list_objects` | List objects in a bucket |
| `head_object` | Get object metadata |
| `create_multipart_upload` | Initiate a multipart upload |
| `upload_part` | Upload a part of a multipart upload |
| `complete_multipart_upload` | Complete a multipart upload |
| `abort_multipart_upload` | Abort a multipart upload |
| `restore_object` | Restore an archived object |
| `lifecycle_transition` | Transition object to different storage class |
| `select_object_content` | Query object content with SQL |
| `wait` | Advance simulation time (for storage cost calculation) |
| `set_bucket_versioning` | Enable/disable bucket versioning |
| `delete_object_version` | Delete a specific object version |
| `replicate_object` | Cross-region replication |

## Cost Categories

The simulator calculates costs across these categories:

- **Storage**: Per-GB-month charges based on storage class
- **Operations**: Per-request charges (PUT, GET, LIST, etc.)
- **Data Transfer**: Egress charges for downloads
- **Retrieval**: Charges for retrieving archived objects
- **Early Deletion**: Penalties for deleting objects before minimum duration
- **Lifecycle Transitions**: Charges for transitioning between storage classes

## Example Workload

```json
{
  "operations": [
    {
      "timestamp": "2024-01-01T00:00:00Z",
      "operation": "put_object",
      "bucket": "my-bucket",
      "key": "data/file.bin",
      "size_bytes": 1073741824,
      "storage_class": "STANDARD"
    },
    {
      "timestamp": "2024-01-15T12:00:00Z",
      "operation": "get_object",
      "bucket": "my-bucket",
      "key": "data/file.bin"
    },
    {
      "timestamp": "2024-02-01T00:00:00Z",
      "operation": "wait",
      "bucket": "_",
      "reason": "Calculate one month of storage"
    }
  ]
}
```

## Example Pricing Rules

```toml
[provider]
name = "aws-s3"
region = "us-east-1"
version = "2024-01"
currency = "USD"

[storage_classes.STANDARD]
storage_price_per_gb_month = "0.023"

[operations.DEFAULT]
put_per_1000 = "0.005"
get_per_1000 = "0.0004"
list_per_1000 = "0.005"
delete_per_1000 = "0"

[data_transfer]
ingress_price_per_gb = "0"
egress_price_per_gb = "0.09"
```

## Documentation

- [Input Format Specification](docs/INPUT_FORMAT.md) - Complete operation format documentation
- [Pricing Rules Format](docs/RULES_FORMAT.md) - Complete pricing rules documentation

## Project Structure

```
cloud-billing-sim/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library root
│   ├── engine/           # Simulation engine
│   │   ├── simulator.rs  # Core simulation logic
│   │   └── state.rs      # Storage state tracking
│   ├── operations/       # Operation types and parsing
│   ├── pricing/          # Pricing rules and calculations
│   └── types/            # Core types (Money, Bytes, etc.)
├── examples/             # Example pricing rules and workloads
│   ├── aws-s3-us-east-1.toml
│   ├── backblaze-b2.toml
│   ├── cloudflare-r2.toml
│   ├── azure-blob.toml
│   └── sample-workload.json
├── docs/                 # Documentation
│   ├── INPUT_FORMAT.md
│   └── RULES_FORMAT.md
└── benches/              # Benchmarks
```

## Development

### Running Tests

```bash
cargo test
```

### Running Benchmarks

```bash
cargo bench
```

### Code Coverage

```bash
cargo +nightly llvm-cov --branch --html
```

## License

MIT License. See [LICENSE](LICENSE) for details.
