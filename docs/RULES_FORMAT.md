# Pricing Rules Format Specification

This document specifies the TOML format for cloud storage pricing rules.

## Overview

Pricing rules define how storage operations are costed. Rules are provider-specific and capture:

- Storage costs (per GB-month, with optional tiering)
- Operation costs (per 1000 requests)
- Data transfer costs (ingress/egress)
- Lifecycle transition costs
- Special features (minimum sizes, early deletion penalties, retrieval tiers)

## File Structure

```toml
[provider]
name = "provider-name"
region = "region-id"
version = "2024-01"
currency = "USD"

[storage_classes.CLASS_NAME]
# Storage class configuration

[operations.DEFAULT]
# Default operation pricing

[operations.CLASS_NAME]
# Class-specific operation pricing (overrides DEFAULT)

[data_transfer]
# Data transfer pricing

[lifecycle_transitions.FROM_to_TO]
# Lifecycle transition pricing
```

---

## Provider Section

Required metadata about the pricing rules.

```toml
[provider]
name = "aws-s3"           # Required: Provider identifier
region = "us-east-1"      # Optional: Region (for region-specific pricing)
version = "2024-01"       # Optional: Pricing version or effective date
currency = "USD"          # Optional: Currency code (default: "USD")
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | String | Yes | — | Provider identifier (e.g., `"aws-s3"`, `"backblaze-b2"`) |
| `region` | String | No | — | AWS region or equivalent |
| `version` | String | No | — | Pricing version for tracking changes |
| `currency` | String | No | `"USD"` | ISO 4217 currency code |

---

## Storage Classes Section

Define pricing for each storage class. Keys are storage class names (uppercase by convention).

### Basic Storage Class

```toml
[storage_classes.STANDARD]
storage_price_per_gb_month = "0.023"
```

### Storage Class with Tiered Pricing

Tiered pricing uses an array of tiers:

```toml
[storage_classes.STANDARD]
storage_price_per_gb_month = [
    { up_to_gb = 51200, price = "0.023" },    # First 50 TB
    { up_to_gb = 512000, price = "0.022" },   # Next 450 TB
    { price = "0.021" }                        # Over 500 TB (no limit)
]
```

Each tier object:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `up_to_gb` | Integer | No | Maximum GB for this tier (omit for final tier) |
| `price` | String | Yes | Price per GB for this tier |

### Storage Class Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `storage_price_per_gb_month` | String or Array | Yes | — | Price per GB per month |
| `min_billable_size_bytes` | Integer | No | — | Minimum billable object size |
| `min_storage_duration_days` | Integer | No | — | Minimum storage duration (for early deletion) |
| `metadata_overhead_bytes` | Integer | No | — | Additional bytes added to each object |
| `retrieval_price_per_gb` | String | No | — | Base retrieval cost per GB |
| `retrieval_tiers` | Array | No | — | Retrieval speed tier options |
| `intelligent_tiering` | Boolean | No | `false` | Whether class uses intelligent tiering |
| `monitoring_price_per_1000_objects` | String | No | — | Monitoring cost for intelligent tiering |

### Complete Storage Class Example

```toml
[storage_classes.GLACIER_FLEXIBLE_RETRIEVAL]
storage_price_per_gb_month = "0.0036"
min_billable_size_bytes = 40960               # 40 KB minimum
min_storage_duration_days = 90                # 90-day minimum
metadata_overhead_bytes = 40960               # 40 KB metadata overhead

[[storage_classes.GLACIER_FLEXIBLE_RETRIEVAL.retrieval_tiers]]
name = "expedited"
price_per_gb = "0.03"
price_per_1000_requests = "10.00"

[[storage_classes.GLACIER_FLEXIBLE_RETRIEVAL.retrieval_tiers]]
name = "standard"
price_per_gb = "0.01"
price_per_1000_requests = "0.05"

[[storage_classes.GLACIER_FLEXIBLE_RETRIEVAL.retrieval_tiers]]
name = "bulk"
price_per_gb = "0.0025"
price_per_1000_requests = "0.025"
```

### Retrieval Tier Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | Yes | Tier name (`"expedited"`, `"standard"`, `"bulk"`) |
| `price_per_gb` | String | Yes | Cost per GB retrieved |
| `price_per_1000_requests` | String | No | Cost per 1000 retrieval requests |

---

## Operations Section

Define per-operation costs. Use `DEFAULT` for the default pricing, and storage class names for class-specific overrides.

```toml
[operations.DEFAULT]
put_per_1000 = "0.005"
get_per_1000 = "0.0004"
list_per_1000 = "0.005"
delete_per_1000 = "0"
head_per_1000 = "0.0004"
lifecycle_transition_per_1000 = "0.01"

[operations.STANDARD_IA]
# Override for Standard-IA (higher costs)
put_per_1000 = "0.01"
get_per_1000 = "0.001"
list_per_1000 = "0.01"
head_per_1000 = "0.001"
```

### Operation Fields

All fields are optional (default to `"0"` if not specified):

| Field | Type | Description |
|-------|------|-------------|
| `put_per_1000` | String | PUT/COPY/POST request cost per 1000 |
| `get_per_1000` | String | GET/SELECT request cost per 1000 |
| `list_per_1000` | String | LIST request cost per 1000 |
| `delete_per_1000` | String | DELETE request cost per 1000 |
| `head_per_1000` | String | HEAD request cost per 1000 |
| `copy_per_1000` | String | COPY-specific cost (if different from PUT) |
| `lifecycle_transition_per_1000` | String | Lifecycle transition request cost per 1000 |

---

## Data Transfer Section

Define data transfer (ingress/egress) pricing.

### Basic Data Transfer

```toml
[data_transfer]
ingress_price_per_gb = "0"      # Usually free
egress_price_per_gb = "0.09"    # Cost per GB downloaded
```

### Tiered Egress Pricing

```toml
[data_transfer]
ingress_price_per_gb = "0"

egress_price_per_gb = [
    { up_to_gb = 10240, price = "0.09" },     # First 10 TB
    { up_to_gb = 51200, price = "0.085" },    # Next 40 TB
    { up_to_gb = 153600, price = "0.07" },    # Next 100 TB
    { price = "0.05" }                         # Over 150 TB
]
```

### Free Egress Allowances

```toml
[data_transfer]
egress_price_per_gb = "0.01"

# Fixed monthly allowance
free_egress_gb_per_month = 100

# OR: Storage-based allowance (e.g., Backblaze 3x rule)
free_egress_storage_multiplier = "3"
```

### Data Transfer Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `ingress_price_per_gb` | String or Array | No | `"0"` | Upload cost per GB |
| `egress_price_per_gb` | String or Array | No | `"0"` | Download cost per GB |
| `free_egress_gb_per_month` | Integer | No | — | Free egress allowance (GB) |
| `free_egress_storage_multiplier` | String | No | — | Free egress as multiple of storage |

---

## Lifecycle Transitions Section

Define costs for transitioning objects between storage classes.

Key format: `FROM_to_TO` (storage class names separated by `_to_`).

```toml
[lifecycle_transitions.STANDARD_to_STANDARD_IA]
per_1000_requests = "0.01"

[lifecycle_transitions.STANDARD_to_GLACIER_FLEXIBLE_RETRIEVAL]
per_1000_requests = "0.03"

[lifecycle_transitions.STANDARD_to_GLACIER_DEEP_ARCHIVE]
per_1000_requests = "0.05"
```

### Lifecycle Transition Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `per_1000_requests` | String | Yes | Cost per 1000 transition requests |

---

## Price Format

All prices are specified as decimal strings for precision:

```toml
price = "0.023"        # Good: String for exact decimal
price = "0.00099"      # Good: Sub-cent pricing
price = 0.023          # Bad: Floating point (precision loss)
```

---

## Complete Examples

### Minimal Example (Backblaze B2)

```toml
[provider]
name = "backblaze-b2"
version = "2024-01"
currency = "USD"

[storage_classes.STANDARD]
storage_price_per_gb_month = "0.006"

[operations.DEFAULT]
put_per_1000 = "0"
get_per_1000 = "0.0004"
list_per_1000 = "0"
head_per_1000 = "0.004"
delete_per_1000 = "0"

[data_transfer]
ingress_price_per_gb = "0"
egress_price_per_gb = "0.01"
free_egress_storage_multiplier = "3"
```

### Full Example (AWS S3)

See `examples/aws-s3-us-east-1.toml` for a complete AWS S3 pricing configuration with:

- Multiple storage classes (Standard, IA, Glacier tiers)
- Tiered storage pricing
- Tiered egress pricing
- Class-specific operation costs
- Retrieval tier pricing
- Lifecycle transition costs

---

## Validation

The simulator validates rules on load:

1. Provider name must be non-empty
2. At least one storage class must be defined
3. All prices must be valid decimal strings
4. Tiered prices must be in ascending order by `up_to_gb`
5. Storage class names are normalized to uppercase

---

## Extending Rules

### Adding a New Provider

1. Create a new TOML file in `examples/`
2. Define at minimum: `provider`, one `storage_class`, and `operations.DEFAULT`
3. Add data transfer pricing if applicable

### Adding Custom Fields

The format is designed for extensibility. Unknown fields are ignored, allowing provider-specific extensions:

```toml
[storage_classes.CUSTOM_CLASS]
storage_price_per_gb_month = "0.01"
# Provider-specific field (ignored by core simulator)
replication_factor = 3
```

---

## Format Rationale and Limitations

### Why TOML?

TOML was chosen for:

1. **Human readability** — Pricing rules need human review and editing
2. **Strong typing** — Distinguishes strings, integers, arrays, and tables
3. **Hierarchical structure** — Natural fit for storage class → operation pricing
4. **Comment support** — Critical for documenting pricing assumptions
5. **Ecosystem support** — Well-supported in Rust (serde integration)

### Current Limitations

The current format **cannot express**:

1. **Conditional pricing** — "If account type is Enterprise, apply 20% discount"
2. **Time-based pricing** — "On-peak vs off-peak rates"
3. **Cross-field dependencies** — "Egress price depends on source storage class"
4. **Complex volume discounts** — "Committed use discounts" or "sustained use discounts"
5. **Geographic egress pricing** — "Egress to EU costs more than egress to US"
6. **Promotional/credit rules** — "First 12 months free tier"

### When Expression Language May Be Needed

Consider adding an expression language if:

- You need dynamic pricing based on runtime context (account type, time of day)
- You need to model complex discount programs
- You need cross-region or multi-destination egress pricing
- You need to express "if-then-else" pricing logic

Potential approaches:

1. **Embedded DSL** — Extend TOML with `expr = "..."` fields
2. **CEL (Common Expression Language)** — Google's expression language
3. **Lua/Rhai scripting** — Embedded scripting for complex rules
4. **YAML with anchors** — Template-based approach

For now, the static TOML format covers the majority of use cases for the three major providers (AWS S3, Backblaze B2, Cloudflare R2).
