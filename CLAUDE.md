# Rust Development Style Guide

Copyright (c) 2026 Geoffrey Washburn. Licensed under MIT.

This document provides detailed guidance and examples for Rust development practices.

---

## Table of Contents

1. [Coverage Analysis](#coverage-analysis)
2. [Testing](#testing)
   - [Doc Tests](#doc-tests)
   - [Mutation Testing](#mutation-testing)
   - [Property-Based Testing with Proptest](#property-based-testing-with-proptest)
   - [Formal Verification with Kani](#formal-verification-with-kani)
   - [Test Execution](#test-execution)
   - [Testability and Mocking](#testability-and-mocking)
   - [Test Organization](#test-organization)
3. [Documentation](#documentation)
4. [Logging and Tracing](#logging-and-tracing)
5. [Benchmarking and Profiling](#benchmarking-and-profiling)
6. [Linting and Formatting](#linting-and-formatting)
7. [Cargo.toml Configuration](#cargotoml-configuration)
8. [Rust Style](#rust-style)
   - [Error Handling](#error-handling)
   - [Trait Derivation](#trait-derivation)
   - [Unsafe Usage Policy](#unsafe-usage-policy)
   - [API Design Patterns](#api-design-patterns)
   - [Async and Concurrency](#async-and-concurrency)
   - [Code Review Practices](#code-review-practices)
9. [Dependency Management](#dependency-management)
10. [Nix Flake Integration](#nix-flake-integration)
11. [Development Tools](#development-tools)
12. [Research Before Implementation](#research-before-implementation)
13. [Quick Reference](#quick-reference)
14. [Regular Maintenance Checklist](#regular-maintenance-checklist)
15. [Summary Checklist](#summary-checklist)

---

## Coverage Analysis

### Required Tools

```bash
# Install cargo-llvm-cov (the ONLY coverage tool to use)
cargo install cargo-llvm-cov

# NEVER use cargo-tarpaulin except for cargo-isotarp interoperability
# cargo install cargo-tarpaulin  # Only if needed for isotarp
```

### Why cargo-llvm-cov?

- Uses LLVM's native instrumentation for accurate coverage
- Supports branch coverage when using Rust nightly
- More accurate than source-based coverage tools
- Better integration with LLVM toolchain

### Running Coverage with Branch Analysis

Branch coverage requires Rust nightly. Switch toolchains as needed:

```bash
# Switch to nightly for branch coverage
rustup override set nightly

# Run coverage with branch analysis
cargo +nightly llvm-cov --branch --all-features

# Generate HTML report with branch coverage
cargo +nightly llvm-cov --branch --html --open

# Generate lcov format for CI integration
cargo +nightly llvm-cov --branch --lcov --output-path lcov.info

# Switch back to stable/MSRV for regular development
rustup override set stable
```

### Modified Condition/Decision Coverage (MC/DC)

Aim for MC/DC coverage levels. This means:

1. Every decision point has taken all possible outcomes
2. Every condition in a decision has been shown to independently affect the outcome

See: https://en.wikipedia.org/wiki/Modified_condition/decision_coverage

Example of MC/DC-aware test design:

```rust
// Function to test
fn complex_condition(a: bool, b: bool, c: bool) -> bool {
    (a && b) || c
}

#[cfg(test)]
mod tests {
    use super::*;

    // MC/DC requires showing each condition independently affects outcome
    // For (a && b) || c, we need:

    #[test]
    fn mcdc_a_affects_outcome() {
        // a changes outcome when b=true, c=false
        assert!(!complex_condition(false, true, false));
        assert!(complex_condition(true, true, false));
    }

    #[test]
    fn mcdc_b_affects_outcome() {
        // b changes outcome when a=true, c=false
        assert!(!complex_condition(true, false, false));
        assert!(complex_condition(true, true, false));
    }

    #[test]
    fn mcdc_c_affects_outcome() {
        // c changes outcome when a && b is false
        assert!(!complex_condition(false, false, false));
        assert!(complex_condition(false, false, true));
    }
}
```

### Coverage Exclusions

#### Flagging Test Code

Test code should never count against coverage:

```rust
#[cfg(test)]
mod tests {
    // This entire module is excluded from coverage by #[cfg(test)]
}
```

#### Excluding Production Code

If you must exclude production code, **document thoroughly**:

```rust
/// Handles the case where the system is shutting down.
///
/// # Coverage Exclusion Rationale
///
/// This function is excluded from coverage analysis because:
/// 1. It only executes during process termination
/// 2. The shutdown signal handler cannot be reliably triggered in tests
/// 3. Testing would require spawning separate processes, which is done
///    in integration tests at tests/integration/shutdown.rs
/// 4. The logic is trivial (log and exit) with no branches
///
/// Verified manually on: 2024-01-15
/// Verified by: @username
/// Issue: #456
#[cfg_attr(coverage_nightly, coverage(off))]
fn handle_shutdown_signal() {
    tracing::info!("Received shutdown signal, exiting...");
    std::process::exit(0);
}
```

For `llvm-cov` specifically:

```rust
// For entire functions
#[cfg_attr(coverage_nightly, coverage(off))]
fn excluded_function() { }

// For specific lines (less preferred)
fn partially_covered() {
    normal_code();

    #[cfg_attr(coverage_nightly, coverage(off))]
    {
        // COVERAGE EXCLUSION: Platform-specific code that only runs on Windows
        // Tested via CI on Windows runners. See .github/workflows/windows.yml
        platform_specific_code();
    }
}
```

---

## Testing

This section covers all testing practices including doc tests, mutation testing, property-based testing, formal verification, test execution, and testability design.

### Doc Tests

Documentation tests serve dual purposes: they verify code examples work correctly and provide tested documentation for users.

#### Running Doc Tests

```bash
# Run doc tests only
cargo test --doc

# Run doc tests with all features
cargo test --doc --all-features

# Run doc tests for a specific module
cargo test --doc -- module_name
```

#### Writing Effective Doc Tests

Doc tests should demonstrate typical usage and be self-contained:

```rust
/// Calculates the factorial of a number.
///
/// # Examples
///
/// ```
/// use my_crate::factorial;
///
/// assert_eq!(factorial(0), 1);
/// assert_eq!(factorial(5), 120);
/// ```
///
/// # Panics
///
/// Panics if the result would overflow `u64`.
///
/// ```should_panic
/// use my_crate::factorial;
///
/// factorial(100); // This overflows
/// ```
pub fn factorial(n: u64) -> u64 {
    (1..=n).product()
}
```

#### Doc Test Attributes

Use attributes to control doc test behavior:

```rust
/// Example that shouldn't be run (compile-only check):
/// ```no_run
/// let server = start_server(); // Would block forever
/// ```
///
/// Example showing code that won't compile:
/// ```compile_fail
/// let x: i32 = "not a number";
/// ```
///
/// Example to ignore in tests:
/// ```ignore
/// // Platform-specific code
/// ```
///
/// Hide setup code from documentation:
/// ```
/// # use my_crate::Config;
/// # let config = Config::default();
/// let result = config.process();
/// assert!(result.is_ok());
/// ```
```

#### Integration with Coverage

Doc tests are included in coverage analysis:

```bash
# Run coverage including doc tests
cargo +nightly llvm-cov --doctests --branch
```

### Mutation Testing

#### Installation

```bash
cargo install cargo-mutants
```

#### Usage

```bash
# Run mutation testing
cargo mutants

# Run with specific timeout
cargo mutants --timeout 60

# Run on specific modules
cargo mutants -- src/lib.rs

# Generate report
cargo mutants --output mutations.json
```

#### Exclusion Policy

**NEVER add cargo-mutants exclusions without documentation.**

If you must exclude code, use this pattern:

```rust
// MUTANTS EXCLUSION: This comparison is tested via integration tests in
// tests/integration/auth_flow.rs because it requires external service mocking
// that cargo-mutants cannot provide. See issue #123 for discussion.
#[mutants::skip]
fn authenticate_external(token: &str) -> Result<User, AuthError> {
    // ...
}
```

For `mutants.toml` exclusions:

```toml
# mutants.toml

# EXCLUSION: Performance-critical hot loop where mutations cause timeout.
# Covered by criterion benchmarks in benches/hot_path.rs that verify
# correctness through performance characteristics.
[[exclude]]
function = "process_batch_inner"
reason = "Timeout issues; verified via benchmark correctness checks"

# EXCLUSION: FFI boundary code that cargo-mutants cannot instrument correctly.
# Covered by integration tests in tests/ffi_integration.rs.
[[exclude]]
function = "ffi_callback_wrapper"
reason = "FFI boundary; see tests/ffi_integration.rs"
```

### Property-Based Testing with Proptest

#### Installation

Add to `Cargo.toml`:

```toml
[dev-dependencies]
proptest = "1"
```

#### Always Start with Proptest

When writing unit tests, **always try starting with proptest** before falling back to example-based tests.

#### Creating Strategies

Define strategies in `tests/strategies.rs` (NOT in `src/`):

```rust
// tests/strategies.rs

use proptest::prelude::*;
use your_crate::{UserId, Email, Age, UserConfig};

/// Strategy for generating valid user IDs (positive non-zero integers)
pub fn user_id_strategy() -> impl Strategy<Value = UserId> {
    (1..=u64::MAX).prop_map(UserId)
}

/// Strategy for generating valid email addresses
pub fn email_strategy() -> impl Strategy<Value = Email> {
    // Local part: alphanumeric, 1-64 chars
    // Domain: alphanumeric with dots, 1-255 chars
    (
        "[a-zA-Z0-9._%+-]{1,64}",
        "[a-zA-Z0-9.-]{1,63}\\.[a-zA-Z]{2,}"
    )
        .prop_map(|(local, domain)| {
            Email::new_unchecked(format!("{}@{}", local, domain))
        })
}

/// Strategy for valid ages (0-150)
pub fn age_strategy() -> impl Strategy<Value = Age> {
    (0u8..=150).prop_map(Age)
}

/// Composite strategy for UserConfig
pub fn user_config_strategy() -> impl Strategy<Value = UserConfig> {
    (
        user_id_strategy(),
        email_strategy(),
        age_strategy(),
        any::<bool>(),  // is_active
        prop::collection::vec(any::<String>(), 0..10),  // tags
    )
        .prop_map(|(id, email, age, is_active, tags)| {
            UserConfig { id, email, age, is_active, tags }
        })
}
```

#### Using Strategies in Tests

```rust
// tests/user_tests.rs

mod strategies;

use proptest::prelude::*;
use strategies::{user_id_strategy, user_config_strategy};

proptest! {
    /// Property: serialization round-trips correctly
    #[test]
    fn user_config_roundtrip(config in user_config_strategy()) {
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: UserConfig = serde_json::from_str(&serialized).unwrap();
        prop_assert_eq!(config, deserialized);
    }

    /// Property: user ID is always preserved through operations
    #[test]
    fn user_id_preserved(id in user_id_strategy()) {
        let user = User::new(id);
        prop_assert_eq!(user.id(), id);
    }

    /// Property: age validation rejects invalid ages
    #[test]
    fn invalid_age_rejected(age in 151u8..=u8::MAX) {
        let result = Age::try_new(age);
        prop_assert!(result.is_err());
    }
}
```

#### Unifying Tests with Proptest

Instead of writing many similar example-based tests, parameterize with proptest:

```rust
// BAD: Many similar tests
#[test]
fn parse_positive_int() { assert!(parse("42").is_ok()); }
#[test]
fn parse_zero() { assert!(parse("0").is_ok()); }
#[test]
fn parse_large_int() { assert!(parse("999999").is_ok()); }

// GOOD: Single parameterized property test
proptest! {
    #[test]
    fn parse_valid_integers(n in 0i64..1_000_000) {
        let s = n.to_string();
        prop_assert!(parse(&s).is_ok());
        prop_assert_eq!(parse(&s).unwrap(), n);
    }
}
```

### Formal Verification with Kani

#### Installation

```bash
cargo install --locked kani-verifier
kani setup
```

#### Implementing Arbitrary for Types

Implement `kani::Arbitrary` for all appropriate bounded types:

```rust
use kani::Arbitrary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedCounter {
    value: u8,  // 0-100
}

impl BoundedCounter {
    pub const MAX: u8 = 100;

    pub fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX {
            Some(Self { value })
        } else {
            None
        }
    }

    pub fn increment(&mut self) -> bool {
        if self.value < Self::MAX {
            self.value += 1;
            true
        } else {
            false
        }
    }
}

#[cfg(kani)]
impl Arbitrary for BoundedCounter {
    fn any() -> Self {
        let value: u8 = kani::any();
        kani::assume(value <= Self::MAX);
        Self { value }
    }
}

#[cfg(kani)]
mod verification {
    use super::*;

    #[kani::proof]
    fn verify_increment_bounds() {
        let mut counter = BoundedCounter::any();
        let old_value = counter.value;

        if counter.increment() {
            assert!(counter.value == old_value + 1);
            assert!(counter.value <= BoundedCounter::MAX);
        } else {
            assert!(old_value == BoundedCounter::MAX);
            assert!(counter.value == old_value);
        }
    }

    #[kani::proof]
    fn verify_new_validates() {
        let value: u8 = kani::any();
        match BoundedCounter::new(value) {
            Some(counter) => assert!(value <= BoundedCounter::MAX),
            None => assert!(value > BoundedCounter::MAX),
        }
    }
}
```

#### Running Kani

```bash
# Run all Kani proofs
cargo kani

# Run specific proof
cargo kani --harness verify_increment_bounds

# With more detailed output
cargo kani --visualize
```

### Test Execution

#### cargo-nextest Setup

```bash
cargo install cargo-nextest
```

Create `.config/nextest.toml`:

```toml
[profile.default]
retries = 0
test-threads = "num-cpus"
fail-fast = false
status-level = "pass"
final-status-level = "flaky"

[profile.ci]
retries = 2
fail-fast = true
```

#### Running Tests

```bash
# Run all tests with nextest
cargo nextest run

# Run with specific profile
cargo nextest run --profile ci

# Run specific test
cargo nextest run test_name

# List tests
cargo nextest list
```

### Testability and Mocking

#### Design for Testability

Use traits to abstract I/O and external dependencies:

```rust
// Define traits for external dependencies
pub trait FileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError>;
    fn write(&self, path: &Path, data: &[u8]) -> Result<(), IoError>;
    fn exists(&self, path: &Path) -> bool;
}

pub trait HttpClient {
    fn get(&self, url: &str) -> Result<Response, HttpError>;
    fn post(&self, url: &str, body: &[u8]) -> Result<Response, HttpError>;
}

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

pub trait RandomSource {
    fn gen_u64(&mut self) -> u64;
}

// Production implementations
pub struct RealFileSystem;
impl FileSystem for RealFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        std::fs::read(path).map_err(IoError::from)
    }
    // ...
}

// Use generics in your code
pub struct Service<F: FileSystem, H: HttpClient, C: Clock> {
    fs: F,
    http: H,
    clock: C,
}

impl<F: FileSystem, H: HttpClient, C: Clock> Service<F, H, C> {
    pub fn process(&self) -> Result<(), Error> {
        let data = self.fs.read(Path::new("config.json"))?;
        let response = self.http.post("https://api.example.com", &data)?;
        let timestamp = self.clock.now();
        // ...
        Ok(())
    }
}
```

#### Test Fakes

```rust
// tests/common/fakes.rs

use std::collections::HashMap;
use std::cell::RefCell;

pub struct FakeFileSystem {
    files: RefCell<HashMap<PathBuf, Vec<u8>>>,
}

impl FakeFileSystem {
    pub fn new() -> Self {
        Self { files: RefCell::new(HashMap::new()) }
    }

    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self {
        self.files.borrow_mut().insert(path.into(), content.into());
        self
    }
}

impl FileSystem for FakeFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        self.files.borrow()
            .get(path)
            .cloned()
            .ok_or(IoError::NotFound)
    }

    fn write(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        self.files.borrow_mut().insert(path.to_owned(), data.to_vec());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }
}

pub struct FakeClock {
    now: DateTime<Utc>,
}

impl FakeClock {
    pub fn fixed(time: DateTime<Utc>) -> Self {
        Self { now: time }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}
```

### Test Organization

#### Directory Structure

```
my-crate/
├── src/
│   ├── lib.rs
│   ├── types.rs
│   └── processing.rs
├── tests/
│   ├── common/
│   │   └── mod.rs          # Shared test utilities
│   ├── strategies/
│   │   ├── mod.rs          # Strategy re-exports
│   │   ├── user.rs         # User-related strategies
│   │   └── config.rs       # Config-related strategies
│   ├── integration_tests.rs
│   └── property_tests.rs
├── benches/
│   └── benchmarks.rs
└── Cargo.toml
```

#### Test Helpers Location

**Put all test utilities in `tests/`, NOT in `src/`:**

```rust
// tests/common/mod.rs
use your_crate::Database;

/// Creates a test database with sample data
pub fn setup_test_db() -> Database {
    let db = Database::in_memory();
    db.seed_test_data();
    db
}

/// Asserts two floating point values are approximately equal
pub fn assert_approx_eq(a: f64, b: f64, epsilon: f64) {
    assert!(
        (a - b).abs() < epsilon,
        "Values not approximately equal: {} vs {} (epsilon: {})",
        a, b, epsilon
    );
}
```

```rust
// tests/strategies/user.rs
use proptest::prelude::*;
use your_crate::User;

pub fn valid_username() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_]{2,31}"
}

pub fn user_strategy() -> impl Strategy<Value = User> {
    (valid_username(), any::<u32>())
        .prop_map(|(name, id)| User::new(id, name))
}
```

---

## Documentation

This section covers rustdoc conventions and best practices for documenting Rust code.

### Documentation Requirements

All public items should be documented. Enforce this with:

```rust
// In lib.rs or main.rs
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
```

### Rustdoc Conventions

#### Standard Documentation Sections

Follow this order for documentation sections:

```rust
/// Brief one-line description of the function.
///
/// Longer description that can span multiple paragraphs. Explain
/// the purpose, behavior, and any important details.
///
/// # Arguments
///
/// * `input` - Description of the input parameter
/// * `config` - Configuration options for processing
///
/// # Returns
///
/// Description of the return value.
///
/// # Errors
///
/// Describe when this function returns an error:
///
/// * [`ProcessError::InvalidInput`] - When input validation fails
/// * [`ProcessError::IoError`] - When file operations fail
///
/// # Panics
///
/// This function panics if:
///
/// * The input slice is empty
/// * The configuration is invalid
///
/// # Safety
///
/// (For unsafe functions) Describe the invariants the caller must uphold.
///
/// # Examples
///
/// ```
/// use my_crate::{process, Config};
///
/// let config = Config::default();
/// let result = process("input data", &config)?;
/// assert_eq!(result.len(), 10);
/// # Ok::<(), my_crate::ProcessError>(())
/// ```
///
/// # See Also
///
/// * [`process_batch`] - For processing multiple inputs
/// * [`Config`] - Configuration options
pub fn process(input: &str, config: &Config) -> Result<Output, ProcessError> {
    // ...
}
```

#### Testable Examples Requirement

**All public functions and methods should include a code snippet example that is testable when possible.** This ensures documentation stays in sync with implementation.

```rust
/// Parses a duration string into seconds.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use my_crate::parse_duration;
///
/// assert_eq!(parse_duration("30s").unwrap(), 30);
/// assert_eq!(parse_duration("5m").unwrap(), 300);
/// assert_eq!(parse_duration("2h").unwrap(), 7200);
/// ```
///
/// Invalid input returns an error:
///
/// ```
/// use my_crate::parse_duration;
///
/// assert!(parse_duration("invalid").is_err());
/// assert!(parse_duration("").is_err());
/// ```
pub fn parse_duration(s: &str) -> Result<u64, ParseError> {
    // ...
}
```

#### Module-Level Documentation

Document modules with an overview and examples:

```rust
//! # User Management
//!
//! This module provides user authentication and authorization.
//!
//! ## Overview
//!
//! The user management system supports:
//!
//! - User registration and login
//! - Role-based access control
//! - Session management
//!
//! ## Examples
//!
//! ```
//! use my_crate::user::{User, AuthService};
//!
//! let auth = AuthService::new();
//! let user = auth.authenticate("username", "password")?;
//! assert!(user.has_role("admin"));
//! # Ok::<(), my_crate::AuthError>(())
//! ```
//!
//! ## Feature Flags
//!
//! - `oauth` - Enables OAuth2 authentication
//! - `ldap` - Enables LDAP integration
```

#### Intra-Doc Links

Use intra-doc links to reference other items:

```rust
/// Creates a new [`Config`] with default settings.
///
/// This is equivalent to calling [`Config::builder()`] and then
/// [`ConfigBuilder::build()`].
///
/// For advanced configuration, see the [configuration guide](crate::guide::config).
///
/// # See Also
///
/// * [`Config::from_env()`] - Load from environment variables
/// * [`Config::from_file()`] - Load from a configuration file
pub fn default_config() -> Config {
    Config::default()
}
```

### Generating Documentation

```bash
# Generate documentation
cargo doc

# Generate and open in browser
cargo doc --open

# Include private items
cargo doc --document-private-items

# Generate with all features
cargo doc --all-features
```

---

## Logging and Tracing

Use the `tracing` ecosystem for structured, contextual logging.

### Setup

Add to `Cargo.toml`:

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

[dev-dependencies]
tracing-test = "0.2"
```

### Initialization

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn init_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            // Default to info level, debug for our crate
            "info,my_crate=debug".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();
}
```

### Log Levels

Use appropriate levels consistently:

```rust
use tracing::{trace, debug, info, warn, error};

fn process_request(request: &Request) -> Result<Response, Error> {
    // TRACE: Very detailed, high-volume diagnostic info
    trace!(request_id = %request.id, "parsing request body");

    // DEBUG: Diagnostic information useful during development
    debug!(user_id = %request.user_id, "authenticated user");

    // INFO: Significant events in normal operation
    info!(
        endpoint = %request.path,
        method = %request.method,
        "handling request"
    );

    // WARN: Potentially problematic situations
    if request.payload_size > LARGE_PAYLOAD_THRESHOLD {
        warn!(
            size = request.payload_size,
            threshold = LARGE_PAYLOAD_THRESHOLD,
            "large payload received"
        );
    }

    // ERROR: Error conditions that should be investigated
    if let Err(e) = validate(&request) {
        error!(error = %e, request_id = %request.id, "validation failed");
        return Err(e);
    }

    Ok(response)
}
```

### Structured Fields

Use structured fields for machine-parseable logs:

```rust
use tracing::{info, instrument, Span};

// Named fields with values
info!(
    user_id = %user.id,
    email = %user.email,
    role = ?user.role,  // Uses Debug formatting
    "user logged in"
);

// Dynamic field names
info!(
    { format!("custom_{}", key) } = %value,
    "dynamic field"
);
```

### Spans for Context

Use spans to group related operations:

```rust
use tracing::{info_span, Instrument};

async fn handle_request(request: Request) -> Result<Response, Error> {
    let span = info_span!(
        "handle_request",
        request_id = %request.id,
        user_id = %request.user_id,
    );

    async {
        let data = fetch_data(&request).await?;
        let response = process_data(data)?;
        Ok(response)
    }
    .instrument(span)
    .await
}
```

### The `#[instrument]` Macro

Automatically create spans for functions:

```rust
use tracing::instrument;

#[instrument(skip(password), fields(user_id))]
async fn authenticate(username: &str, password: &str) -> Result<User, AuthError> {
    let user = lookup_user(username).await?;

    // Record the user_id in the span
    Span::current().record("user_id", &user.id);

    verify_password(&user, password)?;
    Ok(user)
}

#[instrument(level = "debug", ret, err)]
fn parse_config(input: &str) -> Result<Config, ParseError> {
    // `ret` logs the return value, `err` logs any error
    toml::from_str(input)
}
```

### Testing with Tracing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_logging() {
        process_item("test");

        // Assert logs were emitted
        assert!(logs_contain("processing item"));
    }
}
```

### JSON Output for Production

```rust
use tracing_subscriber::fmt::format::JsonFields;

fn init_production_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
        )
        .init();
}
```

---

## Benchmarking and Profiling

### Required Tools

```bash
# Basic benchmarking
cargo install hyperfine

# Flamegraph generation
cargo install flamegraph

# Causal profiling (requires system dependencies)
# On Debian/Ubuntu:
# apt install python3-docutils libelfin-dev
cargo install coz
```

### Criterion Setup

Add to `Cargo.toml`:

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "my_benchmark"
harness = false
```

Create `benches/my_benchmark.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use your_crate::process_data;

fn benchmark_process_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("process_data");

    for size in [100, 1000, 10000].iter() {
        let data = generate_test_data(*size);
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &data,
            |b, data| b.iter(|| process_data(black_box(data))),
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_process_data);
criterion_main!(benches);
```

### Running Benchmarks

```bash
# Run criterion benchmarks
cargo bench

# Basic timing with hyperfine
hyperfine --warmup 3 'cargo run --release -- process input.txt'

# Compare implementations
hyperfine --warmup 3 \
    'cargo run --release --features=impl-a -- process input.txt' \
    'cargo run --release --features=impl-b -- process input.txt'
```

### Flamegraph Profiling

```bash
# Generate flamegraph (requires root on Linux, or dtrace on macOS)
cargo flamegraph --bench my_benchmark

# For specific binary
cargo flamegraph --bin my_app -- --input large_file.txt

# View the generated flamegraph.svg in a browser
```

### Causal Profiling with coz

Add to your code:

```rust
use coz;

fn hot_function() {
    coz::scope!("hot_function");
    // ... computation ...
}

fn outer_loop() {
    for item in items {
        process(item);
        coz::progress!();  // Mark throughput progress
    }
}
```

Run with:

```bash
cargo build --release
coz run --- ./target/release/my_app
```

---

## Linting and Formatting

### Clippy Configuration

Create `clippy.toml`:

```toml
avoid-breaking-exported-api = false
cognitive-complexity-threshold = 25
too-many-arguments-threshold = 7
```

Create or update `.cargo/config.toml`:

```toml
[target.'cfg(all())']
rustflags = [
    "-Wclippy::all",
    "-Wclippy::pedantic",
    "-Wclippy::nursery",
    "-Wclippy::cargo",
    "-Wclippy::unwrap_used",
    "-Wclippy::expect_used",
    "-Wclippy::panic",
    "-Wclippy::todo",
    "-Aclippy::must_use_candidate",
    "-Aclippy::missing_errors_doc",
]
```

### Rustfmt Configuration

Create `rustfmt.toml`:

```toml
edition = "2021"
max_width = 100
tab_spaces = 4
use_small_heuristics = "Default"
imports_granularity = "Module"
group_imports = "StdExternalCrate"
reorder_imports = true
```

### Running Lints

```bash
# Run clippy
cargo clippy --all-targets --all-features

# Format code
cargo fmt

# Check formatting without modifying
cargo fmt -- --check
```

---

## Cargo.toml Configuration

### Complete Example

```toml
[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"  # Minimum supported Rust version
authors = ["Geoffrey Washburn <email@example.com>"]
license = "MIT"
description = "A brief description of what this crate does"
repository = "https://github.com/username/my-crate"
documentation = "https://docs.rs/my-crate"
readme = "README.md"
keywords = ["keyword1", "keyword2"]
categories = ["category1"]

[dependencies]
thiserror = "1"
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
proptest = "1"
criterion = { version = "0.5", features = ["html_reports"] }
tokio-test = "0.4"

[features]
default = []

[[bench]]
name = "benchmarks"
harness = false

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
unwrap_used = "warn"
expect_used = "warn"
```

### License File

Create `LICENSE`:

```
MIT License

Copyright (c) 2026 Geoffrey Washburn

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Rust Style

This section covers Rust coding conventions including error handling patterns, trait derivation guidelines, and policies around unsafe code.

### Error Handling

#### Never Use panic!, todo!, or unwrap()

```rust
// BAD: Can panic
fn get_user(id: u64) -> User {
    users.get(&id).unwrap()  // NEVER do this
}

// BAD: Incomplete code
fn process() {
    todo!()  // NEVER leave in production code
}

// GOOD: Proper error handling
fn get_user(id: u64) -> Result<User, UserError> {
    users.get(&id)
        .cloned()
        .ok_or(UserError::NotFound(id))
}
```

#### Define Proper Error Types

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UserError {
    #[error("user not found: {0}")]
    NotFound(u64),

    #[error("invalid username: {reason}")]
    InvalidUsername { reason: String },

    #[error("database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("permission denied for user {user_id} on resource {resource}")]
    PermissionDenied { user_id: u64, resource: String },
}

// Implement standard traits
impl UserError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}
```

#### Use Non-Panicking Operations

```rust
// BAD: Can panic
let first = vec[0];
let value = map["key"];
let parsed: i32 = string.parse().unwrap();

// GOOD: Cannot panic
let first = vec.first().ok_or(Error::EmptyVec)?;
let value = map.get("key").ok_or(Error::KeyNotFound)?;
let parsed: i32 = string.parse().map_err(Error::ParseInt)?;

// GOOD: Use checked arithmetic
let sum = a.checked_add(b).ok_or(Error::Overflow)?;
let product = a.checked_mul(b).ok_or(Error::Overflow)?;
```

### Trait Derivation

#### Always Derive Debug

```rust
// GOOD: Always derive Debug at minimum
#[derive(Debug)]
pub struct Config {
    pub name: String,
    pub value: u64,
}

// GOOD: Derive additional traits as appropriate
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(u64);

// GOOD: For ordered types
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(u8);

// GOOD: For serializable types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub content: String,
}
```

#### Guidelines for Trait Selection

| Trait | When to Derive |
|-------|----------------|
| `Debug` | **Always** - required for all types |
| `Clone` | When values need to be duplicated |
| `PartialEq`, `Eq` | When equality comparison makes sense |
| `Hash` | When type will be used as HashMap key |
| `PartialOrd`, `Ord` | When ordering makes sense |
| `Default` | When a sensible default exists |
| `Serialize`, `Deserialize` | For data interchange types |

### Unsafe Usage Policy

#### Policy: Avoid Unsafe

**Never use `unsafe` unless there is no alternative.** If you must:

```rust
/// Converts a byte slice to a string without UTF-8 validation.
///
/// # Safety
///
/// This function is unsafe because:
/// 1. The caller must ensure `bytes` contains valid UTF-8 data
/// 2. Invalid UTF-8 will cause undefined behavior in string operations
///
/// # Why unsafe is necessary here
///
/// We receive data from a trusted FFI boundary (libfoo v2.3+) that guarantees
/// UTF-8 encoding. Validating would add O(n) overhead on every call in a
/// hot path (see benchmark results in benches/string_conversion.rs showing
/// 3x slowdown). The FFI contract is documented in docs/ffi-contract.md.
///
/// # Alternatives considered
///
/// 1. `String::from_utf8()` - Rejected due to performance (see above)
/// 2. `String::from_utf8_lossy()` - Rejected as it masks bugs in upstream
/// 3. Lazy validation - Rejected as it defers UB rather than preventing it
#[inline]
pub unsafe fn from_trusted_utf8(bytes: &[u8]) -> &str {
    // SAFETY: Caller guarantees valid UTF-8 per function contract
    std::str::from_utf8_unchecked(bytes)
}
```

### API Design Patterns

#### The Builder Pattern

Use builders for types with many optional parameters:

```rust
#[derive(Debug, Clone)]
pub struct ServerConfig {
    host: String,
    port: u16,
    max_connections: usize,
    timeout: Duration,
    tls_enabled: bool,
}

#[derive(Debug, Default)]
pub struct ServerConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
    max_connections: Option<usize>,
    timeout: Option<Duration>,
    tls_enabled: Option<bool>,
}

impl ServerConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = Some(max);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn tls_enabled(mut self, enabled: bool) -> Self {
        self.tls_enabled = Some(enabled);
        self
    }

    pub fn build(self) -> Result<ServerConfig, ConfigError> {
        Ok(ServerConfig {
            host: self.host.ok_or(ConfigError::MissingField("host"))?,
            port: self.port.unwrap_or(8080),
            max_connections: self.max_connections.unwrap_or(100),
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
            tls_enabled: self.tls_enabled.unwrap_or(false),
        })
    }
}

// Usage
let config = ServerConfigBuilder::new()
    .host("localhost")
    .port(3000)
    .tls_enabled(true)
    .build()?;
```

#### The Newtype Pattern

Wrap primitive types for type safety:

```rust
/// User ID with validation and type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(u64);

impl UserId {
    pub fn new(id: u64) -> Option<Self> {
        if id > 0 {
            Some(Self(id))
        } else {
            None
        }
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Email address with validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn new(email: impl Into<String>) -> Result<Self, EmailError> {
        let email = email.into();
        if email.contains('@') && email.len() > 3 {
            Ok(Self(email))
        } else {
            Err(EmailError::Invalid)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Now functions are type-safe
fn send_notification(user: UserId, email: &Email) -> Result<(), Error> {
    // Cannot accidentally pass an email as a user ID
}
```

#### The Type-State Pattern

Use the type system to enforce state transitions:

```rust
use std::marker::PhantomData;

// State markers
pub struct Unvalidated;
pub struct Validated;
pub struct Submitted;

pub struct Form<State> {
    data: FormData,
    _state: PhantomData<State>,
}

impl Form<Unvalidated> {
    pub fn new(data: FormData) -> Self {
        Self {
            data,
            _state: PhantomData,
        }
    }

    pub fn validate(self) -> Result<Form<Validated>, ValidationError> {
        self.data.validate()?;
        Ok(Form {
            data: self.data,
            _state: PhantomData,
        })
    }
}

impl Form<Validated> {
    pub fn submit(self) -> Result<Form<Submitted>, SubmitError> {
        send_to_server(&self.data)?;
        Ok(Form {
            data: self.data,
            _state: PhantomData,
        })
    }
}

impl Form<Submitted> {
    pub fn confirmation_number(&self) -> &str {
        &self.data.confirmation
    }
}

// Usage - compile-time enforcement of valid transitions
let form = Form::new(data);
// form.submit(); // Compile error! Must validate first
let validated = form.validate()?;
let submitted = validated.submit()?;
println!("Confirmation: {}", submitted.confirmation_number());
```

#### Extension Traits

Add methods to external types:

```rust
pub trait StringExt {
    fn truncate_ellipsis(&self, max_len: usize) -> String;
    fn is_blank(&self) -> bool;
}

impl StringExt for str {
    fn truncate_ellipsis(&self, max_len: usize) -> String {
        if self.len() <= max_len {
            self.to_string()
        } else if max_len <= 3 {
            "...".to_string()
        } else {
            format!("{}...", &self[..max_len - 3])
        }
    }

    fn is_blank(&self) -> bool {
        self.trim().is_empty()
    }
}

// Usage
let title = "Very Long Title Here".truncate_ellipsis(10);
assert_eq!(title, "Very Lo...");
```

### Async and Concurrency

#### Tokio Best Practices

Add to `Cargo.toml`:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

#### Avoid Blocking in Async Contexts

```rust
// BAD: Blocking call in async context
async fn bad_read_file(path: &Path) -> io::Result<String> {
    std::fs::read_to_string(path)  // Blocks the executor!
}

// GOOD: Use async file I/O
async fn good_read_file(path: &Path) -> io::Result<String> {
    tokio::fs::read_to_string(path).await
}

// GOOD: Use spawn_blocking for CPU-intensive or blocking code
async fn compute_hash(data: Vec<u8>) -> String {
    tokio::task::spawn_blocking(move || {
        // CPU-intensive work is OK here
        expensive_hash_function(&data)
    })
    .await
    .expect("spawn_blocking failed")
}
```

#### Structured Concurrency

Use `JoinSet` for managing concurrent tasks:

```rust
use tokio::task::JoinSet;

async fn fetch_all_pages(urls: Vec<String>) -> Vec<Result<Page, Error>> {
    let mut set = JoinSet::new();

    for url in urls {
        set.spawn(async move {
            fetch_page(&url).await
        });
    }

    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        match result {
            Ok(page_result) => results.push(page_result),
            Err(join_error) => {
                results.push(Err(Error::TaskPanicked(join_error.to_string())));
            }
        }
    }

    results
}
```

#### Cancellation Safety

Be aware of cancellation points:

```rust
use tokio::select;
use tokio_util::sync::CancellationToken;

async fn cancellable_operation(cancel: CancellationToken) -> Result<Data, Error> {
    select! {
        result = do_work() => result,
        _ = cancel.cancelled() => Err(Error::Cancelled),
    }
}

// Use drop guards for cleanup
struct CleanupGuard {
    resource: Resource,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        // Cleanup runs even if task is cancelled
        self.resource.cleanup();
    }
}

async fn safe_operation() -> Result<(), Error> {
    let resource = acquire_resource().await?;
    let _guard = CleanupGuard { resource };

    // If cancelled here, guard still runs cleanup
    do_something().await?;

    Ok(())
}
```

#### Send and Sync Bounds

Understand when types can cross thread boundaries:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

// BAD: Rc is not Send
async fn bad_shared_state() {
    let state = Rc::new(RefCell::new(0));
    // tokio::spawn(async move { ... }) // Won't compile!
}

// GOOD: Arc + Mutex for shared async state
async fn good_shared_state() {
    let state = Arc::new(Mutex::new(0));
    let state_clone = Arc::clone(&state);

    tokio::spawn(async move {
        let mut guard = state_clone.lock().await;
        *guard += 1;
    });
}

// GOOD: Use channels for communication
use tokio::sync::mpsc;

async fn channel_based() {
    let (tx, mut rx) = mpsc::channel(100);

    tokio::spawn(async move {
        tx.send("message").await.ok();
    });

    while let Some(msg) = rx.recv().await {
        println!("Received: {}", msg);
    }
}
```

#### Deadlock Prevention

```rust
// BAD: Potential deadlock with nested locks
async fn deadlock_prone() {
    let a = Arc::new(Mutex::new(0));
    let b = Arc::new(Mutex::new(0));

    // Task 1 locks a then b
    // Task 2 locks b then a
    // = Deadlock!
}

// GOOD: Always acquire locks in consistent order
async fn deadlock_free() {
    let resources = Arc::new(Mutex::new((ResourceA::new(), ResourceB::new())));

    // Single lock protects both resources
    let mut guard = resources.lock().await;
    guard.0.update();
    guard.1.update();
}

// GOOD: Use timeouts
use tokio::time::{timeout, Duration};

async fn with_timeout() -> Result<(), Error> {
    timeout(Duration::from_secs(5), acquire_lock())
        .await
        .map_err(|_| Error::LockTimeout)??;
    Ok(())
}
```

### Code Review Practices

#### Regular Reviews For

1. **Dead code** - Remove unused functions, types, and modules
2. **Code duplication** - Unify similar code with parameters or generics
3. **Unification opportunities** - Similar logic that could share implementation

#### Example: Unifying Similar Code

```rust
// BEFORE: Duplicated logic
fn process_user_event(event: UserEvent) -> Result<(), Error> {
    validate_event(&event)?;
    log_event(&event);
    store_event(&event)?;
    notify_subscribers(&event)?;
    Ok(())
}

fn process_system_event(event: SystemEvent) -> Result<(), Error> {
    validate_event(&event)?;
    log_event(&event);
    store_event(&event)?;
    notify_subscribers(&event)?;
    Ok(())
}

// AFTER: Unified with trait
trait Event: Validate + Log + Store + Notify {}

fn process_event<E: Event>(event: E) -> Result<(), Error> {
    event.validate()?;
    event.log();
    event.store()?;
    event.notify_subscribers()?;
    Ok(())
}
```

#### Unifying Tests

```rust
// BEFORE: Many similar tests
#[test]
fn parse_json_succeeds() { ... }
#[test]
fn parse_yaml_succeeds() { ... }
#[test]
fn parse_toml_succeeds() { ... }

// AFTER: Parameterized property test
proptest! {
    #[test]
    fn parse_succeeds_for_valid_input(
        format in prop_oneof![Just(Format::Json), Just(Format::Yaml), Just(Format::Toml)],
        config in config_strategy()
    ) {
        let serialized = serialize(&config, format);
        let parsed = parse(&serialized, format)?;
        prop_assert_eq!(config, parsed);
    }
}
```

---

## Dependency Management

### cargo-deny Setup

Install and configure:

```bash
cargo install cargo-deny
cargo deny init
```

Configure `deny.toml`:

```toml
[advisories]
db-path = "~/.cargo/advisory-db"
vulnerability = "deny"
unmaintained = "warn"
yanked = "deny"
notice = "warn"

[licenses]
unlicensed = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
]
copyleft = "deny"
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "deny"
highlight = "all"

# Deny specific problematic crates
deny = [
    # { name = "problematic-crate", reason = "Security issues" }
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

### Other Dependency Tools

```bash
# Install tools
cargo install cargo-udeps
cargo install cargo-audit

# Find unused dependencies
cargo +nightly udeps --all-targets

# Check for security vulnerabilities
cargo audit

# Run cargo-deny checks
cargo deny check
```

---

## Nix Flake Integration

### Basic flake.nix

```nix
{
  description = "My Rust crate";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        rustNightly = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "llvm-tools-preview" ];
        };

        # Build inputs
        buildInputs = with pkgs; [
          openssl
        ];

        # Native build inputs
        nativeBuildInputs = with pkgs; [
          pkg-config
          rustToolchain
        ];

      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "my-crate";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          inherit buildInputs nativeBuildInputs;
        };

        devShells.default = pkgs.mkShell {
          inherit buildInputs;
          nativeBuildInputs = nativeBuildInputs ++ (with pkgs; [
            # Development tools
            cargo-watch
            cargo-edit
            cargo-audit
            cargo-deny
            cargo-nextest
            cargo-llvm-cov
            cargo-mutants
            hyperfine
          ]);

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        # Separate shell for coverage with nightly
        devShells.coverage = pkgs.mkShell {
          inherit buildInputs;
          nativeBuildInputs = [
            rustNightly
            pkgs.cargo-llvm-cov
          ];
        };
      }
    );
}
```

### Usage

```bash
# Enter development shell
nix develop

# Enter coverage shell (nightly)
nix develop .#coverage

# Build the package
nix build

# Run the package
nix run
```

---

## Development Tools

### Required Installations

```bash
# Core tools
cargo install cargo-llvm-cov
cargo install cargo-mutants
cargo install cargo-nextest
cargo install cargo-audit
cargo install cargo-deny
cargo install cargo-udeps

# Benchmarking and profiling
cargo install hyperfine
cargo install flamegraph
cargo install coz  # Requires libelfin and python3-docutils

# Development
cargo install rust-analyzer
cargo install cargo-expand  # Macro expansion debugging

# Optional: Tokio debugging
cargo install tokio-console
```

### cargo-expand for Macro Debugging

Use `cargo-expand` to see what macros expand to:

```bash
# Expand all macros in the crate
cargo expand

# Expand a specific module
cargo expand module_name

# Expand a specific item
cargo expand module_name::function_name

# Expand with specific features
cargo expand --features some_feature

# Output to a file for comparison
cargo expand > expanded.rs
```

Example workflow for debugging a derive macro:

```rust
// Your code
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub name: String,
    pub value: u64,
}
```

```bash
# See what the derives expand to
cargo expand --lib | grep -A 50 "impl.*Config"
```

This is invaluable for:
- Understanding what procedural macros generate
- Debugging macro hygiene issues
- Learning how derive macros work
- Troubleshooting compile errors in macro-generated code

### rust-analyzer Configuration

Create `.vscode/settings.json` or equivalent:

```json
{
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.check.allTargets": true,
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.procMacro.enable": true,
  "rust-analyzer.diagnostics.experimental.enable": true
}
```

### tokio-console Setup

For async debugging, add to `Cargo.toml`:

```toml
[dependencies]
console-subscriber = { version = "0.2", optional = true }

[features]
tokio-console = ["console-subscriber"]
```

In your main.rs:

```rust
#[cfg(feature = "tokio-console")]
fn init_console() {
    console_subscriber::init();
}

#[tokio::main]
async fn main() {
    #[cfg(feature = "tokio-console")]
    init_console();

    // ... rest of application
}
```

Run with:

```bash
RUSTFLAGS="--cfg tokio_unstable" cargo run --features tokio-console
tokio-console  # In another terminal
```

---

## Research Before Implementation

### Always Search First

Before implementing new functionality:

```bash
# Search for existing approaches
# - crates.io for existing crates
# - GitHub for similar implementations
# - Rust forums and Reddit for discussions
# - Official documentation
```

### What to Look For

1. **Existing crates** that solve the problem
2. **Common patterns** used in the Rust ecosystem
3. **Pitfalls** others have encountered
4. **Performance considerations** documented by others
5. **Security implications** of the approach

---

## Quick Reference

### Common Commands

```bash
# Development cycle
cargo fmt                           # Format code
cargo clippy --all-targets          # Lint code
cargo nextest run                   # Run tests
cargo doc --open                    # Generate and view docs

# Coverage (requires nightly)
rustup override set nightly
cargo llvm-cov --branch --html --doctests
rustup override set stable

# Quality checks
cargo mutants                       # Mutation testing
cargo audit                         # Security vulnerabilities
cargo deny check                    # License and dependency policy
cargo +nightly udeps                # Find unused dependencies

# Debugging
cargo expand                        # View macro expansions
RUST_BACKTRACE=1 cargo run          # Enable backtraces
cargo test -- --nocapture           # See test output

# Benchmarking
cargo bench                         # Run criterion benchmarks
cargo flamegraph                    # Generate flamegraph

# Formal verification
cargo kani                          # Run Kani proofs
```

### Cargo.toml Quick Setup

```toml
[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT"

[dependencies]
thiserror = "1"
tracing = "0.1"

[dev-dependencies]
proptest = "1"
criterion = { version = "0.5", features = ["html_reports"] }

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = "warn"
pedantic = "warn"
unwrap_used = "warn"
```

### Error Handling Pattern

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

fn process(input: &str) -> Result<Output, MyError> {
    let value = input.parse()
        .map_err(|_| MyError::InvalidInput(input.to_string()))?;
    Ok(value)
}
```

### Proptest Quick Start

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn my_property(input in "\\w+") {
        let result = process(&input);
        prop_assert!(result.is_ok());
    }
}
```

### Tracing Quick Start

```rust
use tracing::{info, instrument};

#[instrument(skip(password))]
fn login(username: &str, password: &str) -> Result<User, Error> {
    info!(username, "attempting login");
    // ...
}
```

### Builder Pattern Quick Start

```rust
#[derive(Default)]
pub struct ConfigBuilder {
    field: Option<String>,
}

impl ConfigBuilder {
    pub fn field(mut self, value: impl Into<String>) -> Self {
        self.field = Some(value.into());
        self
    }

    pub fn build(self) -> Result<Config, Error> {
        Ok(Config {
            field: self.field.ok_or(Error::MissingField)?,
        })
    }
}
```

### Async Pattern Quick Start

```rust
use tokio::task::JoinSet;

async fn parallel_fetch(urls: Vec<String>) -> Vec<Result<Data, Error>> {
    let mut set = JoinSet::new();
    for url in urls {
        set.spawn(async move { fetch(&url).await });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        results.push(res.unwrap_or_else(|e| Err(e.into())));
    }
    results
}
```

---

## Regular Maintenance Checklist

Run these regularly during development:

```bash
# Format and lint
cargo fmt
cargo clippy --all-targets --all-features

# Run tests
cargo nextest run
cargo test --doc

# Coverage analysis (switch to nightly first)
rustup override set nightly
cargo llvm-cov --branch --html --doctests
rustup override set stable

# Mutation testing
cargo mutants

# Security audit
cargo audit
cargo deny check

# Unused dependencies
cargo +nightly udeps

# Run benchmarks
cargo bench

# Generate flamegraph for profiling
cargo flamegraph --bench benchmarks

# Formal verification
cargo kani

# Generate and review documentation
cargo doc --open
```

---

## Summary Checklist

### Coverage and Testing
- [ ] Coverage with `cargo-llvm-cov` only (nightly for branch coverage)
- [ ] Doc tests for all public functions with testable examples
- [ ] Mutation testing with `cargo-mutants` (document any exclusions)
- [ ] Property tests with `proptest` as primary testing approach
- [ ] Kani verification for bounded types with `Arbitrary` implementations
- [ ] Fast test execution with `cargo-nextest`
- [ ] Test utilities in `tests/` not `src/`
- [ ] Coverage exclusions documented with rationale

### Documentation
- [ ] `#![deny(missing_docs)]` enabled
- [ ] All public items documented with rustdoc
- [ ] Testable code examples in documentation
- [ ] Module-level documentation with overview and examples
- [ ] Intra-doc links for cross-references

### Logging and Observability
- [ ] Structured logging with `tracing` crate
- [ ] Appropriate log levels (trace, debug, info, warn, error)
- [ ] Spans for contextual grouping of operations
- [ ] `#[instrument]` on key functions

### Code Style
- [ ] No panics, todos, or unwraps; proper `Result` handling
- [ ] Custom error types with `thiserror`
- [ ] `Debug` trait on all types; other traits as appropriate
- [ ] No `unsafe` without thorough documentation
- [ ] Builder pattern for complex configuration
- [ ] Newtype pattern for type safety
- [ ] Code reviewed for dead code and duplication

### Async and Concurrency
- [ ] No blocking calls in async contexts
- [ ] `spawn_blocking` for CPU-intensive work
- [ ] Proper cancellation handling
- [ ] `Arc`/`Mutex` instead of `Rc`/`RefCell` for shared async state
- [ ] Consistent lock ordering to prevent deadlocks

### Project Configuration
- [ ] Linting with `clippy`; formatting with `rustfmt`
- [ ] Proper `Cargo.toml` with MSRV, metadata, and MIT license
- [ ] Benchmarks with `criterion`; profiling with `flamegraph` and `coz`
- [ ] Dependency management with `cargo-deny`, `cargo-audit`, `cargo-udeps`
- [ ] Testable design with trait abstractions for I/O
- [ ] Nix flake for installation

### Maintenance
- [ ] README kept up-to-date
- [ ] Research performed before implementation
- [ ] `cargo-expand` available for macro debugging
