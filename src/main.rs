//! Cloud Billing Simulator CLI

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use cloud_billing_sim::{engine, operations, pricing};

/// Cloud billing simulator for object storage costs.
#[derive(Parser)]
#[command(name = "cloud-billing-sim")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Simulate costs for a workload
    Simulate {
        /// Path to pricing rules TOML file
        #[arg(short, long)]
        rules: PathBuf,

        /// Path to operations JSON file
        #[arg(short, long)]
        operations: PathBuf,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: OutputFormat,

        /// Show per-object cost breakdown
        #[arg(long)]
        per_object: bool,
    },

    /// Validate pricing rules file
    ValidateRules {
        /// Path to pricing rules TOML file
        #[arg(short, long)]
        rules: PathBuf,
    },

    /// Validate operations file
    ValidateOperations {
        /// Path to operations JSON file
        #[arg(short, long)]
        operations: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("Unknown format: {s}")),
        }
    }
}

fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = match cli.verbose {
        0 => "warn,cloud_billing_sim=info",
        1 => "info,cloud_billing_sim=debug",
        _ => "debug,cloud_billing_sim=trace",
    };

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let result = match cli.command {
        Commands::Simulate {
            rules,
            operations,
            format,
            per_object,
        } => run_simulation(&rules, &operations, format, per_object),
        Commands::ValidateRules { rules } => validate_rules(&rules),
        Commands::ValidateOperations { operations } => validate_operations(&operations),
    };

    if let Err(e) = result {
        error!("{e}");
        std::process::exit(1);
    }
}

fn run_simulation(
    rules_path: &PathBuf,
    ops_path: &PathBuf,
    format: OutputFormat,
    per_object: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Loading pricing rules from {}", rules_path.display());
    let rules = pricing::load_rules(rules_path)?;

    info!("Loading operations from {}", ops_path.display());
    let ops = operations::load_operations(ops_path)?;

    info!("Simulating {} operations", ops.operations.len());

    let mut sim = engine::Simulator::new(rules);
    let report = sim.simulate(&ops)?;

    match format {
        OutputFormat::Text => {
            println!("{report}");

            if per_object && !report.object_costs.is_empty() {
                println!("\nPer-Object Costs:");
                println!("-----------------");
                let mut objects: Vec<_> = report.object_costs.iter().collect();
                objects.sort_by(|a, b| b.1.total.cmp(&a.1.total));

                for (path, costs) in objects {
                    println!("  {}: {}", path, costs.total);
                    for (category, cost) in &costs.by_category {
                        println!("    {}: {}", category, cost);
                    }
                }
            }

            println!("\nStatistics:");
            println!("-----------");
            println!("  Total operations: {}", report.stats.total_operations);
            println!("  Bytes uploaded:   {}", report.stats.bytes_uploaded);
            println!("  Bytes downloaded: {}", report.stats.bytes_downloaded);
            println!("  Peak storage:     {}", report.stats.peak_storage);
            println!("  Final storage:    {}", report.stats.final_storage);
            println!("  Objects created:  {}", report.stats.objects_created);
            println!("  Objects deleted:  {}", report.stats.objects_deleted);
        }
        OutputFormat::Json => {
            // Serialize report to JSON
            let json_report = JsonReport {
                total_cost: report.total_cost.as_decimal().to_string(),
                breakdown: JsonBreakdown {
                    storage: report.breakdown.total_storage.as_decimal().to_string(),
                    operations: report.breakdown.total_operations.as_decimal().to_string(),
                    data_transfer: report
                        .breakdown
                        .data_transfer_egress
                        .as_decimal()
                        .to_string(),
                    retrieval: report.breakdown.retrieval.as_decimal().to_string(),
                    early_deletion: report
                        .breakdown
                        .early_deletion_penalties
                        .as_decimal()
                        .to_string(),
                    lifecycle_transitions: report
                        .breakdown
                        .lifecycle_transitions
                        .as_decimal()
                        .to_string(),
                },
                stats: JsonStats {
                    total_operations: report.stats.total_operations,
                    bytes_uploaded: report.stats.bytes_uploaded.as_bytes(),
                    bytes_downloaded: report.stats.bytes_downloaded.as_bytes(),
                    peak_storage: report.stats.peak_storage.as_bytes(),
                    final_storage: report.stats.final_storage.as_bytes(),
                    objects_created: report.stats.objects_created,
                    objects_deleted: report.stats.objects_deleted,
                },
            };
            println!("{}", serde_json::to_string_pretty(&json_report)?);
        }
    }

    Ok(())
}

#[derive(serde::Serialize)]
struct JsonReport {
    total_cost: String,
    breakdown: JsonBreakdown,
    stats: JsonStats,
}

#[derive(serde::Serialize)]
struct JsonBreakdown {
    storage: String,
    operations: String,
    data_transfer: String,
    retrieval: String,
    early_deletion: String,
    lifecycle_transitions: String,
}

#[derive(serde::Serialize)]
struct JsonStats {
    total_operations: u64,
    bytes_uploaded: u64,
    bytes_downloaded: u64,
    peak_storage: u64,
    final_storage: u64,
    objects_created: u64,
    objects_deleted: u64,
}

fn validate_rules(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Validating pricing rules from {}", path.display());

    let rules = pricing::load_rules(path)?;

    println!("Pricing rules valid!");
    println!("  Provider: {}", rules.provider.name);
    if let Some(region) = &rules.provider.region {
        println!("  Region: {region}");
    }
    println!("  Storage classes: {}", rules.storage_classes.len());
    for class in rules.storage_classes.keys() {
        println!("    - {class}");
    }

    Ok(())
}

fn validate_operations(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Validating operations from {}", path.display());

    let ops = operations::load_operations(path)?;

    println!("Operations file valid!");
    println!("  Operations: {}", ops.operations.len());

    if let Some((start, end)) = ops.time_range() {
        println!("  Time range: {} to {}", start, end);
    }

    // Count by type
    let mut by_type = std::collections::HashMap::new();
    for op in &ops.operations {
        let type_name = match &op.kind {
            operations::OperationKind::PutObject { .. } => "PutObject",
            operations::OperationKind::GetObject { .. } => "GetObject",
            operations::OperationKind::DeleteObject => "DeleteObject",
            operations::OperationKind::CopyObject { .. } => "CopyObject",
            operations::OperationKind::ListObjects { .. } => "ListObjects",
            operations::OperationKind::HeadObject => "HeadObject",
            operations::OperationKind::CreateMultipartUpload { .. } => "CreateMultipartUpload",
            operations::OperationKind::UploadPart { .. } => "UploadPart",
            operations::OperationKind::CompleteMultipartUpload { .. } => "CompleteMultipartUpload",
            operations::OperationKind::AbortMultipartUpload { .. } => "AbortMultipartUpload",
            operations::OperationKind::RestoreObject { .. } => "RestoreObject",
            operations::OperationKind::LifecycleTransition { .. } => "LifecycleTransition",
            operations::OperationKind::SelectObjectContent { .. } => "SelectObjectContent",
        };
        *by_type.entry(type_name).or_insert(0u64) += 1;
    }

    println!("  By type:");
    for (op_type, count) in by_type {
        println!("    {op_type}: {count}");
    }

    Ok(())
}
