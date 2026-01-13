//! Benchmarks for cloud-billing-sim
//!
//! Run with: `cargo bench`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use cloud_billing_sim::operations::{Operation, OperationKind, OperationLog};
use cloud_billing_sim::pricing::{
    DataTransferRules, OperationRules, PricingRules, ProviderInfo, StorageClassRules, TieredPrice,
};
use cloud_billing_sim::types::{Bytes, Money, StorageClass};
use cloud_billing_sim::{engine, operations, pricing};

use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::str::FromStr;

// ============================================================================
// Test Data Generation
// ============================================================================

fn sample_pricing_rules() -> PricingRules {
    let mut storage_classes = HashMap::new();
    storage_classes.insert(
        "STANDARD".to_string(),
        StorageClassRules {
            storage_price_per_gb_month: TieredPrice::flat(
                Money::from_str("0.023").ok().unwrap_or(Money::ZERO),
            ),
            min_billable_size_bytes: None,
            min_storage_duration_days: None,
            metadata_overhead_bytes: None,
            retrieval_price_per_gb: None,
            retrieval_tiers: vec![],
            intelligent_tiering: false,
            monitoring_price_per_1000_objects: None,
        },
    );

    let mut operations = HashMap::new();
    operations.insert(
        "DEFAULT".to_string(),
        OperationRules {
            put_per_1000: Money::from_str("0.005").ok(),
            get_per_1000: Money::from_str("0.0004").ok(),
            list_per_1000: Money::from_str("0.005").ok(),
            delete_per_1000: Some(Money::ZERO),
            head_per_1000: Money::from_str("0.0004").ok(),
            lifecycle_transition_per_1000: None,
        },
    );

    PricingRules {
        provider: ProviderInfo {
            name: "benchmark".to_string(),
            region: Some("us-east-1".to_string()),
            version: None,
            currency: "USD".to_string(),
        },
        storage_classes,
        operations,
        data_transfer: DataTransferRules::default(),
        lifecycle_transitions: HashMap::new(),
    }
}

fn generate_operations(count: usize) -> OperationLog {
    let base_time = Utc::now();
    let ops: Vec<Operation> = (0..count)
        .map(|i| Operation {
            timestamp: base_time + Duration::seconds(i as i64),
            bucket: "benchmark-bucket".to_string(),
            key: Some(format!("file-{i}.txt")),
            kind: if i % 3 == 0 {
                OperationKind::PutObject {
                    size_bytes: 1024 * 1024, // 1 MB
                    storage_class: StorageClass::new("STANDARD"),
                }
            } else if i % 3 == 1 {
                OperationKind::GetObject {
                    bytes_transferred: Some(1024 * 1024),
                    retrieval_tier: None,
                }
            } else {
                OperationKind::HeadObject
            },
        })
        .collect();

    OperationLog {
        operations: ops,
        metadata: None,
    }
}

fn sample_operations_json() -> String {
    r#"{
        "operations": [
            {
                "timestamp": "2024-01-01T00:00:00Z",
                "operation": "put_object",
                "bucket": "test-bucket",
                "key": "file1.txt",
                "size_bytes": 1048576,
                "storage_class": "STANDARD"
            },
            {
                "timestamp": "2024-01-01T00:01:00Z",
                "operation": "get_object",
                "bucket": "test-bucket",
                "key": "file1.txt"
            },
            {
                "timestamp": "2024-01-01T00:02:00Z",
                "operation": "put_object",
                "bucket": "test-bucket",
                "key": "file2.txt",
                "size_bytes": 2097152,
                "storage_class": "STANDARD"
            }
        ]
    }"#
    .to_string()
}

fn sample_rules_toml() -> String {
    r#"
[provider]
name = "aws-s3"
region = "us-east-1"
currency = "USD"

[storage_classes.STANDARD]
storage_price_per_gb_month = "0.023"

[storage_classes.STANDARD_IA]
storage_price_per_gb_month = "0.0125"
min_billable_size_bytes = 131072
min_storage_duration_days = 30

[operations.DEFAULT]
put_per_1000 = "0.005"
get_per_1000 = "0.0004"
list_per_1000 = "0.005"
delete_per_1000 = "0"
head_per_1000 = "0.0004"

[data_transfer]
ingress_price_per_gb = "0"
egress_price_per_gb = "0.09"
"#
    .to_string()
}

// ============================================================================
// Parsing Benchmarks
// ============================================================================

fn bench_parse_operations(c: &mut Criterion) {
    let json = sample_operations_json();

    c.bench_function("parse_operations/small", |b| {
        b.iter(|| operations::parse_operations(black_box(&json)))
    });
}

fn generate_operations_json(count: usize) -> String {
    let ops: Vec<String> = (0..count)
        .map(|i| {
            format!(
                r#"{{
                "timestamp": "2024-01-{:02}T{:02}:{:02}:00Z",
                "operation": "put_object",
                "bucket": "test-bucket",
                "key": "file{}.txt",
                "size_bytes": {}
            }}"#,
                (i / 1440) % 28 + 1,
                (i / 60) % 24,
                i % 60,
                i,
                1024 * (i + 1)
            )
        })
        .collect();

    format!(r#"{{"operations": [{}]}}"#, ops.join(","))
}

fn bench_parse_operations_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_operations");

    for size in [100, 500, 1000, 5000, 10000].iter() {
        let json = generate_operations_json(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &json, |b, json| {
            b.iter(|| operations::parse_operations(black_box(json)))
        });
    }

    group.finish();
}

fn bench_parse_rules(c: &mut Criterion) {
    let toml = sample_rules_toml();

    c.bench_function("parse_rules/typical", |b| {
        b.iter(|| pricing::parse_rules(black_box(&toml)))
    });
}

// ============================================================================
// Simulation Benchmarks
// ============================================================================

fn bench_simulate_small(c: &mut Criterion) {
    let rules = sample_pricing_rules();
    let log = generate_operations(10);

    c.bench_function("simulate/10_ops", |b| {
        b.iter(|| {
            let mut sim = engine::Simulator::new(rules.clone());
            let _ = sim.simulate(black_box(&log));
            sim.into_report()
        })
    });
}

fn bench_simulate_scaling(c: &mut Criterion) {
    let rules = sample_pricing_rules();

    let mut group = c.benchmark_group("simulate");

    for size in [10, 50, 100, 500, 1000, 5000, 10000].iter() {
        let log = generate_operations(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &log, |b, log| {
            b.iter(|| {
                let mut sim = engine::Simulator::new(rules.clone());
                let _ = sim.simulate(black_box(log));
                sim.into_report()
            })
        });
    }

    group.finish();
}

// ============================================================================
// Tiered Price Calculation Benchmarks
// ============================================================================

fn bench_tiered_price_flat(c: &mut Criterion) {
    let price = TieredPrice::flat(Money::from_str("0.023").ok().unwrap_or(Money::ZERO));

    c.bench_function("tiered_price/flat", |b| {
        b.iter(|| price.calculate_cost(black_box(rust_decimal::Decimal::from(1000))))
    });
}

fn bench_tiered_price_tiered(c: &mut Criterion) {
    use cloud_billing_sim::pricing::PriceTier;

    let price = TieredPrice::tiered(vec![
        PriceTier {
            up_to_gb: Some(51200),
            price: Money::from_str("0.023").ok().unwrap_or(Money::ZERO),
        },
        PriceTier {
            up_to_gb: Some(512000),
            price: Money::from_str("0.022").ok().unwrap_or(Money::ZERO),
        },
        PriceTier {
            up_to_gb: None,
            price: Money::from_str("0.021").ok().unwrap_or(Money::ZERO),
        },
    ]);

    let mut group = c.benchmark_group("tiered_price/tiered");

    // Test at different volumes
    for gb in [100, 50000, 100000, 600000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(gb), gb, |b, &gb| {
            b.iter(|| price.calculate_cost(black_box(rust_decimal::Decimal::from(gb))))
        });
    }

    group.finish();
}

// ============================================================================
// Type Operation Benchmarks
// ============================================================================

fn bench_money_operations(c: &mut Criterion) {
    let a = Money::from_str("123.456789").ok().unwrap_or(Money::ZERO);
    let b = Money::from_str("987.654321").ok().unwrap_or(Money::ZERO);

    let mut group = c.benchmark_group("money");

    group.bench_function("add", |b_iter| {
        b_iter.iter(|| black_box(a) + black_box(b))
    });

    group.bench_function("multiply", |b_iter| {
        b_iter.iter(|| black_box(a) * black_box(rust_decimal::Decimal::from(1000)))
    });

    group.bench_function("from_str", |b_iter| {
        b_iter.iter(|| Money::from_str(black_box("123.456789")))
    });

    group.bench_function("display", |b_iter| {
        b_iter.iter(|| format!("{}", black_box(a)))
    });

    group.finish();
}

fn bench_bytes_operations(c: &mut Criterion) {
    let a = Bytes::from_gb(100);
    let b = Bytes::from_mb(500);

    let mut group = c.benchmark_group("bytes");

    group.bench_function("add", |b_iter| {
        b_iter.iter(|| black_box(a) + black_box(b))
    });

    group.bench_function("as_gb_decimal", |b_iter| {
        b_iter.iter(|| black_box(a).as_gb_decimal())
    });

    group.bench_function("from_gb", |b_iter| {
        b_iter.iter(|| Bytes::from_gb(black_box(100)))
    });

    group.bench_function("display", |b_iter| {
        b_iter.iter(|| format!("{}", black_box(a)))
    });

    group.finish();
}

fn bench_storage_class(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_class");

    group.bench_function("new", |b| {
        b.iter(|| StorageClass::new(black_box("STANDARD")))
    });

    group.bench_function("new_lowercase", |b| {
        b.iter(|| StorageClass::new(black_box("standard")))
    });

    let class_a = StorageClass::new("STANDARD");
    let class_b = StorageClass::new("standard");

    group.bench_function("eq", |b| {
        b.iter(|| black_box(&class_a) == black_box(&class_b))
    });

    group.finish();
}

// ============================================================================
// Data Transfer Calculation Benchmarks
// ============================================================================

fn bench_egress_calculation(c: &mut Criterion) {
    use cloud_billing_sim::pricing::PriceTier;

    let mut rules = DataTransferRules::default();
    rules.egress_price_per_gb = TieredPrice::tiered(vec![
        PriceTier {
            up_to_gb: Some(10240),
            price: Money::from_str("0.09").ok().unwrap_or(Money::ZERO),
        },
        PriceTier {
            up_to_gb: Some(51200),
            price: Money::from_str("0.085").ok().unwrap_or(Money::ZERO),
        },
        PriceTier {
            up_to_gb: Some(153600),
            price: Money::from_str("0.07").ok().unwrap_or(Money::ZERO),
        },
        PriceTier {
            up_to_gb: None,
            price: Money::from_str("0.05").ok().unwrap_or(Money::ZERO),
        },
    ]);

    let mut group = c.benchmark_group("egress_cost");

    for gb in [100, 10000, 50000, 200000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(gb), gb, |b, &gb| {
            let egress = rust_decimal::Decimal::from(gb);
            let storage = rust_decimal::Decimal::from(1000);
            b.iter(|| rules.calculate_egress_cost(black_box(egress), black_box(storage)))
        });
    }

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    parsing,
    bench_parse_operations,
    bench_parse_operations_scaling,
    bench_parse_rules,
);

criterion_group!(
    simulation,
    bench_simulate_small,
    bench_simulate_scaling,
);

criterion_group!(
    pricing_calc,
    bench_tiered_price_flat,
    bench_tiered_price_tiered,
    bench_egress_calculation,
);

criterion_group!(
    types,
    bench_money_operations,
    bench_bytes_operations,
    bench_storage_class,
);

criterion_main!(parsing, simulation, pricing_calc, types);
