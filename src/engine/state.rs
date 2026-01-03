//! Storage state tracking.

use crate::types::{Bytes, StorageClass};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Tracks the state of all objects in storage.
#[derive(Debug, Clone, Default)]
pub struct StorageState {
    /// Objects indexed by bucket/key.
    objects: HashMap<ObjectKey, ObjectState>,

    /// In-progress multipart uploads.
    multipart_uploads: HashMap<String, MultipartUploadState>,
}

/// Unique key for an object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectKey {
    pub bucket: String,
    pub key: String,
}

impl ObjectKey {
    /// Creates a new object key.
    #[must_use]
    pub fn new(bucket: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            key: key.into(),
        }
    }
}

impl std::fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.bucket, self.key)
    }
}

/// State of a single object.
#[derive(Debug, Clone)]
pub struct ObjectState {
    /// Object size in bytes.
    pub size: Bytes,

    /// Current storage class.
    pub storage_class: StorageClass,

    /// When the object was created/uploaded.
    pub created_at: DateTime<Utc>,

    /// When the object was last modified.
    pub last_modified: DateTime<Utc>,

    /// Last time storage was billed to (for incremental billing).
    pub last_billed_at: DateTime<Utc>,

    /// Storage class history for tracking transitions.
    pub class_history: Vec<StorageClassChange>,
}

impl ObjectState {
    /// Creates a new object state.
    #[must_use]
    pub fn new(size: Bytes, storage_class: StorageClass, created_at: DateTime<Utc>) -> Self {
        Self {
            size,
            storage_class: storage_class.clone(),
            created_at,
            last_modified: created_at,
            last_billed_at: created_at,
            class_history: vec![StorageClassChange {
                class: storage_class,
                changed_at: created_at,
            }],
        }
    }

    /// Transitions the object to a new storage class.
    pub fn transition_to(&mut self, new_class: StorageClass, at: DateTime<Utc>) {
        self.storage_class = new_class.clone();
        self.last_modified = at;
        self.class_history.push(StorageClassChange {
            class: new_class,
            changed_at: at,
        });
    }

    /// Returns the number of days stored since creation.
    #[must_use]
    pub fn days_stored(&self, as_of: DateTime<Utc>) -> u32 {
        let duration = as_of.signed_duration_since(self.created_at);
        duration.num_days().try_into().unwrap_or(0)
    }
}

/// Record of a storage class change.
#[derive(Debug, Clone)]
pub struct StorageClassChange {
    pub class: StorageClass,
    pub changed_at: DateTime<Utc>,
}

/// State of an in-progress multipart upload.
#[derive(Debug, Clone)]
pub struct MultipartUploadState {
    pub bucket: String,
    pub key: String,
    pub storage_class: StorageClass,
    pub started_at: DateTime<Utc>,
    pub parts: HashMap<u32, PartState>,
}

/// State of a single part in a multipart upload.
#[derive(Debug, Clone)]
pub struct PartState {
    pub size: Bytes,
    pub uploaded_at: DateTime<Utc>,
}

impl StorageState {
    /// Creates a new empty storage state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or updates an object.
    pub fn put_object(
        &mut self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        size: Bytes,
        storage_class: StorageClass,
        timestamp: DateTime<Utc>,
    ) {
        let obj_key = ObjectKey::new(bucket, key);
        self.objects
            .insert(obj_key, ObjectState::new(size, storage_class, timestamp));
    }

    /// Gets an object's state.
    #[must_use]
    pub fn get_object(&self, bucket: &str, key: &str) -> Option<&ObjectState> {
        let obj_key = ObjectKey {
            bucket: bucket.to_string(),
            key: key.to_string(),
        };
        self.objects.get(&obj_key)
    }

    /// Gets mutable object state.
    pub fn get_object_mut(&mut self, bucket: &str, key: &str) -> Option<&mut ObjectState> {
        let obj_key = ObjectKey {
            bucket: bucket.to_string(),
            key: key.to_string(),
        };
        self.objects.get_mut(&obj_key)
    }

    /// Removes an object.
    pub fn delete_object(&mut self, bucket: &str, key: &str) -> Option<ObjectState> {
        let obj_key = ObjectKey {
            bucket: bucket.to_string(),
            key: key.to_string(),
        };
        self.objects.remove(&obj_key)
    }

    /// Returns all objects.
    pub fn objects(&self) -> impl Iterator<Item = (&ObjectKey, &ObjectState)> {
        self.objects.iter()
    }

    /// Returns total storage by class.
    #[must_use]
    pub fn total_storage_by_class(&self) -> HashMap<StorageClass, Bytes> {
        let mut totals: HashMap<StorageClass, Bytes> = HashMap::new();
        for obj in self.objects.values() {
            *totals.entry(obj.storage_class.clone()).or_default() += obj.size;
        }
        totals
    }

    /// Starts a multipart upload.
    pub fn start_multipart_upload(
        &mut self,
        upload_id: String,
        bucket: String,
        key: String,
        storage_class: StorageClass,
        timestamp: DateTime<Utc>,
    ) {
        self.multipart_uploads.insert(
            upload_id,
            MultipartUploadState {
                bucket,
                key,
                storage_class,
                started_at: timestamp,
                parts: HashMap::new(),
            },
        );
    }

    /// Adds a part to a multipart upload.
    pub fn add_part(
        &mut self,
        upload_id: &str,
        part_number: u32,
        size: Bytes,
        timestamp: DateTime<Utc>,
    ) -> bool {
        if let Some(upload) = self.multipart_uploads.get_mut(upload_id) {
            upload.parts.insert(
                part_number,
                PartState {
                    size,
                    uploaded_at: timestamp,
                },
            );
            true
        } else {
            false
        }
    }

    /// Completes a multipart upload, creating the final object.
    pub fn complete_multipart_upload(
        &mut self,
        upload_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Option<Bytes> {
        let upload = self.multipart_uploads.remove(upload_id)?;

        // Calculate total size from parts
        let total_size: Bytes = upload
            .parts
            .values()
            .map(|p| p.size)
            .fold(Bytes::ZERO, |a, b| a + b);

        // Create the final object
        self.put_object(
            upload.bucket,
            upload.key,
            total_size,
            upload.storage_class,
            timestamp,
        );

        Some(total_size)
    }

    /// Aborts a multipart upload.
    pub fn abort_multipart_upload(&mut self, upload_id: &str) -> bool {
        self.multipart_uploads.remove(upload_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_lifecycle() {
        let mut state = StorageState::new();
        let now = Utc::now();

        // Put object
        state.put_object(
            "bucket",
            "key",
            Bytes::from_mb(1),
            StorageClass::new("STANDARD"),
            now,
        );

        assert!(state.get_object("bucket", "key").is_some());

        // Delete object
        let removed = state.delete_object("bucket", "key");
        assert!(removed.is_some());
        assert!(state.get_object("bucket", "key").is_none());
    }

    #[test]
    fn multipart_upload_lifecycle() {
        let mut state = StorageState::new();
        let now = Utc::now();

        state.start_multipart_upload(
            "upload-1".into(),
            "bucket".into(),
            "key".into(),
            StorageClass::new("STANDARD"),
            now,
        );

        state.add_part("upload-1", 1, Bytes::from_mb(5), now);
        state.add_part("upload-1", 2, Bytes::from_mb(5), now);

        let total = state.complete_multipart_upload("upload-1", now);
        assert_eq!(total, Some(Bytes::from_mb(10)));
        assert!(state.get_object("bucket", "key").is_some());
    }
}
