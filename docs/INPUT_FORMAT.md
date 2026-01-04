# Operation Input Format Specification

This document specifies the JSON format for operation logs that the cloud billing simulator processes.

## Overview

Operations are provided as a JSON file containing an array of timestamped storage operations. The simulator processes these operations chronologically to compute costs.

## File Structure

```json
{
  "operations": [
    { ... },
    { ... }
  ],
  "metadata": {
    "description": "Optional description",
    "source": "Optional source system"
  }
}
```

## Top-Level Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `operations` | Array | Yes | List of operations in chronological order |
| `metadata` | Object | No | Optional metadata about the operation log |

### Metadata Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `description` | String | No | Human-readable description of the workload |
| `source` | String | No | Identifier for the system that generated this log |

## Operation Object

Every operation has these common fields:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `timestamp` | String | Yes | ISO 8601 timestamp (e.g., `"2024-01-15T10:30:00Z"`) |
| `operation` | String | Yes | Operation type (see below) |
| `bucket` | String | Yes | Bucket name |
| `key` | String | No | Object key (path within bucket) |

Additional fields depend on the operation type.

---

## Operation Types

### `put_object`

Upload or create an object.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `size_bytes` | Integer | No | `0` | Size of the object in bytes |
| `storage_class` | String | No | `"STANDARD"` | Storage class for the object |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "operation": "put_object",
  "bucket": "my-bucket",
  "key": "path/to/file.txt",
  "size_bytes": 1048576,
  "storage_class": "STANDARD"
}
```

---

### `get_object`

Download or retrieve an object.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `bytes_transferred` | Integer | No | Full object size | Bytes actually transferred (for partial reads) |
| `retrieval_tier` | String | No | Provider default | Retrieval speed tier: `"expedited"`, `"standard"`, or `"bulk"` |

**Example:**
```json
{
  "timestamp": "2024-01-15T11:00:00Z",
  "operation": "get_object",
  "bucket": "my-bucket",
  "key": "path/to/file.txt",
  "bytes_transferred": 1048576
}
```

**Example with retrieval tier (for archived objects):**
```json
{
  "timestamp": "2024-01-15T11:00:00Z",
  "operation": "get_object",
  "bucket": "my-bucket",
  "key": "archived-file.txt",
  "retrieval_tier": "bulk"
}
```

---

### `delete_object`

Delete an object.

No additional fields.

**Example:**
```json
{
  "timestamp": "2024-01-15T12:00:00Z",
  "operation": "delete_object",
  "bucket": "my-bucket",
  "key": "path/to/file.txt"
}
```

---

### `copy_object`

Copy an object within the same bucket or across buckets.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `source_key` | String | Yes | — | Key of the source object |
| `source_bucket` | String | No | Same as destination | Source bucket (if different) |
| `storage_class` | String | No | Same as source | Storage class for the copy |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:45:00Z",
  "operation": "copy_object",
  "bucket": "destination-bucket",
  "key": "copied-file.txt",
  "source_key": "original-file.txt",
  "source_bucket": "source-bucket",
  "storage_class": "STANDARD_IA"
}
```

---

### `list_objects`

List objects in a bucket.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `objects_returned` | Integer | No | — | Number of objects in the response |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:00:00Z",
  "operation": "list_objects",
  "bucket": "my-bucket",
  "key": "path/prefix/",
  "objects_returned": 1000
}
```

---

### `head_object`

Get object metadata only.

No additional fields.

**Example:**
```json
{
  "timestamp": "2024-01-15T10:05:00Z",
  "operation": "head_object",
  "bucket": "my-bucket",
  "key": "path/to/file.txt"
}
```

---

### `create_multipart_upload`

Initiate a multipart upload.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `storage_class` | String | No | `"STANDARD"` | Storage class for the upload |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:00:00Z",
  "operation": "create_multipart_upload",
  "bucket": "my-bucket",
  "key": "large-file.bin",
  "storage_class": "STANDARD"
}
```

---

### `upload_part`

Upload a part of a multipart upload.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `upload_id` | String | Yes | — | Multipart upload ID |
| `part_number` | Integer | Yes | — | Part number (1-10000) |
| `size_bytes` | Integer | Yes | — | Size of this part in bytes |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:01:00Z",
  "operation": "upload_part",
  "bucket": "my-bucket",
  "key": "large-file.bin",
  "upload_id": "abc123",
  "part_number": 1,
  "size_bytes": 5242880
}
```

---

### `complete_multipart_upload`

Complete a multipart upload.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `upload_id` | String | Yes | — | Multipart upload ID |
| `total_size_bytes` | Integer | No | — | Total size of the completed object |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:10:00Z",
  "operation": "complete_multipart_upload",
  "bucket": "my-bucket",
  "key": "large-file.bin",
  "upload_id": "abc123",
  "total_size_bytes": 52428800
}
```

---

### `abort_multipart_upload`

Abort a multipart upload.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `upload_id` | String | Yes | — | Multipart upload ID |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:10:00Z",
  "operation": "abort_multipart_upload",
  "bucket": "my-bucket",
  "key": "large-file.bin",
  "upload_id": "abc123"
}
```

---

### `restore_object`

Restore an archived object.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `days` | Integer | Yes | — | Number of days to keep the restored copy |
| `tier` | String | No | `"standard"` | Retrieval tier: `"expedited"`, `"standard"`, or `"bulk"` |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:00:00Z",
  "operation": "restore_object",
  "bucket": "archive-bucket",
  "key": "archived-file.tar.gz",
  "days": 7,
  "tier": "bulk"
}
```

---

### `lifecycle_transition`

Transition an object to a different storage class (typically automated).

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `new_storage_class` | String | Yes | — | Target storage class |

**Example:**
```json
{
  "timestamp": "2024-01-15T00:00:00Z",
  "operation": "lifecycle_transition",
  "bucket": "my-bucket",
  "key": "old-file.txt",
  "new_storage_class": "GLACIER_FLEXIBLE_RETRIEVAL"
}
```

---

### `select_object_content`

Query object content using SQL-like expressions.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `bytes_scanned` | Integer | No | — | Bytes scanned by the query |
| `bytes_returned` | Integer | No | — | Bytes returned in the response |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:00:00Z",
  "operation": "select_object_content",
  "bucket": "data-bucket",
  "key": "dataset.csv",
  "bytes_scanned": 1073741824,
  "bytes_returned": 1048576
}
```

---

### `wait`

Advance time without performing any cloud operation. This is used to calculate storage costs for a specific duration after uploading objects.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `reason` | String | No | — | Optional description of why the wait was added |

**Note:** The `bucket` field is required for format consistency but is ignored for wait operations. Use any value (e.g., `"_"`).

**Example:**
```json
{
  "timestamp": "2024-02-15T00:00:00Z",
  "operation": "wait",
  "bucket": "_",
  "reason": "Calculate 30 days of storage costs"
}
```

**Use case:** Calculate storage costs for objects held for 6 months:
```json
{
  "operations": [
    {
      "timestamp": "2024-01-01T00:00:00Z",
      "operation": "put_object",
      "bucket": "my-bucket",
      "key": "large-archive.tar",
      "size_bytes": 107374182400,
      "storage_class": "GLACIER_DEEP_ARCHIVE"
    },
    {
      "timestamp": "2024-07-01T00:00:00Z",
      "operation": "wait",
      "bucket": "_",
      "reason": "Calculate 6 months of storage costs"
    }
  ]
}
```

---

### `set_bucket_versioning`

Enable or disable versioning on a bucket. When versioning is enabled, updating an object creates a new version instead of replacing the existing one. Previous versions continue to incur storage costs.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `enabled` | Boolean | Yes | — | Whether versioning is enabled |

**Note:** The `key` field is not required for this operation.

**Example:**
```json
{
  "timestamp": "2024-01-01T00:00:00Z",
  "operation": "set_bucket_versioning",
  "bucket": "my-versioned-bucket",
  "enabled": true
}
```

---

### `delete_object_version`

Delete a specific version of an object. This is used when versioning is enabled to remove old versions. Early deletion penalties may apply if the version hasn't met minimum storage duration requirements.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `version_id` | String | Yes | — | Version ID to delete |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:00:00Z",
  "operation": "delete_object_version",
  "bucket": "my-versioned-bucket",
  "key": "documents/report.pdf",
  "version_id": "1704067200000000000"
}
```

---

### `replicate_object`

Replicate an object to another region. This represents cross-region replication and incurs data transfer (egress) costs plus a PUT operation cost in the destination region.

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `destination_region` | String | Yes | — | Target region for the replica |
| `destination_bucket` | String | No | Same as source | Destination bucket name |
| `destination_key` | String | No | Same as source | Destination object key |
| `storage_class` | String | No | Same as source | Storage class for the replica |

**Example:**
```json
{
  "timestamp": "2024-01-15T10:00:00Z",
  "operation": "replicate_object",
  "bucket": "primary-bucket",
  "key": "data/important-file.bin",
  "destination_region": "eu-west-1",
  "destination_bucket": "backup-bucket-eu",
  "storage_class": "STANDARD_IA"
}
```

**Use case:** Cross-region disaster recovery:
```json
{
  "operations": [
    {
      "timestamp": "2024-01-01T00:00:00Z",
      "operation": "put_object",
      "bucket": "us-east-1-primary",
      "key": "critical-data.tar",
      "size_bytes": 10737418240,
      "storage_class": "STANDARD"
    },
    {
      "timestamp": "2024-01-01T00:01:00Z",
      "operation": "replicate_object",
      "bucket": "us-east-1-primary",
      "key": "critical-data.tar",
      "destination_region": "eu-west-1",
      "destination_bucket": "eu-west-1-replica"
    }
  ]
}
```

---

## Complete Example

```json
{
  "metadata": {
    "description": "Daily backup workload",
    "source": "backup-service-v2"
  },
  "operations": [
    {
      "timestamp": "2024-01-15T02:00:00Z",
      "operation": "list_objects",
      "bucket": "backup-bucket",
      "key": "daily/"
    },
    {
      "timestamp": "2024-01-15T02:00:01Z",
      "operation": "put_object",
      "bucket": "backup-bucket",
      "key": "daily/2024-01-15/database.sql.gz",
      "size_bytes": 524288000,
      "storage_class": "STANDARD"
    },
    {
      "timestamp": "2024-01-15T02:05:00Z",
      "operation": "put_object",
      "bucket": "backup-bucket",
      "key": "daily/2024-01-15/files.tar.gz",
      "size_bytes": 1073741824,
      "storage_class": "STANDARD"
    },
    {
      "timestamp": "2024-01-15T03:00:00Z",
      "operation": "lifecycle_transition",
      "bucket": "backup-bucket",
      "key": "daily/2024-01-08/database.sql.gz",
      "new_storage_class": "GLACIER_FLEXIBLE_RETRIEVAL"
    },
    {
      "timestamp": "2024-01-15T10:30:00Z",
      "operation": "get_object",
      "bucket": "backup-bucket",
      "key": "daily/2024-01-15/database.sql.gz",
      "bytes_transferred": 524288000
    }
  ]
}
```

---

## Storage Class Values

Common storage class identifiers:

### AWS S3
- `STANDARD`
- `INTELLIGENT_TIERING`
- `STANDARD_IA`
- `ONEZONE_IA`
- `GLACIER_INSTANT_RETRIEVAL`
- `GLACIER_FLEXIBLE_RETRIEVAL`
- `GLACIER_DEEP_ARCHIVE`

### Backblaze B2
- `STANDARD` (B2 has a single storage class)

### Cloudflare R2
- `STANDARD`
- `INFREQUENT_ACCESS`

---

## Timestamp Format

Timestamps must be in ISO 8601 format with timezone. Examples:

```
2024-01-15T10:30:00Z          # UTC
2024-01-15T10:30:00+00:00     # UTC (explicit offset)
2024-01-15T05:30:00-05:00     # US Eastern
```

The simulator uses UTC internally; all timestamps are converted.

---

## Generating Operation Logs

### From AWS CloudTrail

```bash
# Extract S3 data events from CloudTrail
aws cloudtrail lookup-events \
  --lookup-attributes AttributeKey=EventSource,AttributeValue=s3.amazonaws.com \
  --start-time 2024-01-01 \
  --end-time 2024-01-31 \
  | jq '.Events | map(.CloudTrailEvent | fromjson)'
```

### From S3 Server Access Logs

Parse S3 access logs and convert to the operation format.

### Synthetic Generation

For testing and capacity planning, generate synthetic workloads programmatically.

---

## Validation

The simulator validates operations on load:

1. All timestamps must be valid ISO 8601
2. Bucket names must be non-empty
3. Operation types must be recognized
4. Required fields for each operation type must be present
5. Numeric values must be non-negative

Invalid operations cause load errors with descriptive messages.
