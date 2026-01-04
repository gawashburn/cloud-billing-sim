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

    /// Bucket-level versioning settings.
    versioning_enabled: HashMap<String, bool>,

    /// Noncurrent (versioned) objects.
    /// Key is (bucket, key), value is list of version states.
    noncurrent_versions: HashMap<ObjectKey, Vec<VersionState>>,
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

    /// Version ID (if versioning is enabled).
    pub version_id: Option<String>,

    /// Region where the object is stored.
    pub region: Option<String>,
}

/// State of a noncurrent (versioned) object.
#[derive(Debug, Clone)]
pub struct VersionState {
    /// Version ID.
    pub version_id: String,

    /// Object size in bytes.
    pub size: Bytes,

    /// Storage class.
    pub storage_class: StorageClass,

    /// When this version was created.
    pub created_at: DateTime<Utc>,

    /// When this version became noncurrent.
    pub became_noncurrent_at: DateTime<Utc>,

    /// Last time storage was billed to.
    pub last_billed_at: DateTime<Utc>,
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
            version_id: None,
            region: None,
        }
    }

    /// Creates a new object state with version ID.
    #[must_use]
    pub fn with_version(
        size: Bytes,
        storage_class: StorageClass,
        created_at: DateTime<Utc>,
        version_id: String,
    ) -> Self {
        let mut obj = Self::new(size, storage_class, created_at);
        obj.version_id = Some(version_id);
        obj
    }

    /// Creates a new object state with region.
    #[must_use]
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
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
    pub parts: HashMap<u32, PartState>,
}

/// State of a single part in a multipart upload.
#[derive(Debug, Clone)]
pub struct PartState {
    pub size: Bytes,
}

impl StorageState {
    /// Creates a new empty storage state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables or disables versioning for a bucket.
    pub fn set_versioning(&mut self, bucket: impl Into<String>, enabled: bool) {
        self.versioning_enabled.insert(bucket.into(), enabled);
    }

    /// Checks if versioning is enabled for a bucket.
    #[must_use]
    pub fn is_versioning_enabled(&self, bucket: &str) -> bool {
        self.versioning_enabled.get(bucket).copied().unwrap_or(false)
    }

    /// Generates a version ID for a new object.
    fn generate_version_id(&self, timestamp: DateTime<Utc>) -> String {
        format!("{}", timestamp.timestamp_nanos_opt().unwrap_or(0))
    }

    /// Adds or updates an object, handling versioning if enabled.
    ///
    /// Returns the previous version if it was moved to noncurrent storage.
    pub fn put_object(
        &mut self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        size: Bytes,
        storage_class: StorageClass,
        timestamp: DateTime<Utc>,
    ) -> Option<VersionState> {
        let bucket = bucket.into();
        let key = key.into();
        let obj_key = ObjectKey::new(&bucket, &key);

        let prev_version = if self.is_versioning_enabled(&bucket) {
            // If versioning enabled and object exists, move current to noncurrent
            if let Some(existing) = self.objects.remove(&obj_key) {
                let version = VersionState {
                    version_id: existing
                        .version_id
                        .clone()
                        .unwrap_or_else(|| self.generate_version_id(existing.created_at)),
                    size: existing.size,
                    storage_class: existing.storage_class,
                    created_at: existing.created_at,
                    became_noncurrent_at: timestamp,
                    last_billed_at: existing.last_billed_at,
                };

                self.noncurrent_versions
                    .entry(obj_key.clone())
                    .or_default()
                    .push(version.clone());

                Some(version)
            } else {
                None
            }
        } else {
            None
        };

        // Create the new current version
        let mut new_obj = ObjectState::new(size, storage_class, timestamp);
        if self.is_versioning_enabled(&bucket) {
            new_obj.version_id = Some(self.generate_version_id(timestamp));
        }

        self.objects.insert(obj_key, new_obj);
        prev_version
    }

    /// Adds an object with a specific region.
    pub fn put_object_with_region(
        &mut self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        size: Bytes,
        storage_class: StorageClass,
        timestamp: DateTime<Utc>,
        region: impl Into<String>,
    ) -> Option<VersionState> {
        let bucket = bucket.into();
        let key = key.into();
        let prev = self.put_object(&bucket, &key, size, storage_class, timestamp);

        // Set the region on the newly created object
        if let Some(obj) = self.get_object_mut(&bucket, &key) {
            obj.region = Some(region.into());
        }

        prev
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

    /// Removes an object (current version).
    ///
    /// If versioning is enabled, this creates a delete marker and moves
    /// the current version to noncurrent. Returns the removed state.
    pub fn delete_object(&mut self, bucket: &str, key: &str) -> Option<ObjectState> {
        let obj_key = ObjectKey {
            bucket: bucket.to_string(),
            key: key.to_string(),
        };
        self.objects.remove(&obj_key)
    }

    /// Deletes a specific version of an object.
    ///
    /// Returns the removed version state if found.
    pub fn delete_version(&mut self, bucket: &str, key: &str, version_id: &str) -> Option<VersionState> {
        let obj_key = ObjectKey {
            bucket: bucket.to_string(),
            key: key.to_string(),
        };

        // Check if it's the current version
        if let Some(current) = self.objects.get(&obj_key) {
            if current.version_id.as_deref() == Some(version_id) {
                let removed = self.objects.remove(&obj_key)?;
                return Some(VersionState {
                    version_id: version_id.to_string(),
                    size: removed.size,
                    storage_class: removed.storage_class,
                    created_at: removed.created_at,
                    became_noncurrent_at: removed.last_modified,
                    last_billed_at: removed.last_billed_at,
                });
            }
        }

        // Check noncurrent versions
        if let Some(versions) = self.noncurrent_versions.get_mut(&obj_key) {
            if let Some(pos) = versions.iter().position(|v| v.version_id == version_id) {
                return Some(versions.remove(pos));
            }
        }

        None
    }

    /// Returns all noncurrent versions for an object.
    #[must_use]
    pub fn get_noncurrent_versions(&self, bucket: &str, key: &str) -> &[VersionState] {
        let obj_key = ObjectKey {
            bucket: bucket.to_string(),
            key: key.to_string(),
        };
        self.noncurrent_versions
            .get(&obj_key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Returns an iterator over all noncurrent versions across all objects.
    pub fn all_noncurrent_versions(&self) -> impl Iterator<Item = (&ObjectKey, &VersionState)> {
        self.noncurrent_versions
            .iter()
            .flat_map(|(key, versions)| versions.iter().map(move |v| (key, v)))
    }

    /// Returns mutable access to noncurrent versions for billing updates.
    pub fn noncurrent_versions_mut(
        &mut self,
    ) -> impl Iterator<Item = (&ObjectKey, &mut VersionState)> {
        self.noncurrent_versions
            .iter_mut()
            .flat_map(|(key, versions)| versions.iter_mut().map(move |v| (key, v)))
    }

    /// Returns all objects.
    pub fn objects(&self) -> impl Iterator<Item = (&ObjectKey, &ObjectState)> {
        self.objects.iter()
    }

    /// Returns total storage by class (including noncurrent versions).
    #[must_use]
    pub fn total_storage_by_class(&self) -> HashMap<StorageClass, Bytes> {
        let mut totals: HashMap<StorageClass, Bytes> = HashMap::new();

        // Current objects
        for obj in self.objects.values() {
            *totals.entry(obj.storage_class.clone()).or_default() += obj.size;
        }

        // Noncurrent versions
        for versions in self.noncurrent_versions.values() {
            for version in versions {
                *totals.entry(version.storage_class.clone()).or_default() += version.size;
            }
        }

        totals
    }

    /// Returns total noncurrent version storage by class.
    #[must_use]
    pub fn noncurrent_storage_by_class(&self) -> HashMap<StorageClass, Bytes> {
        let mut totals: HashMap<StorageClass, Bytes> = HashMap::new();
        for versions in self.noncurrent_versions.values() {
            for version in versions {
                *totals.entry(version.storage_class.clone()).or_default() += version.size;
            }
        }
        totals
    }

    /// Returns the count of noncurrent versions.
    #[must_use]
    pub fn noncurrent_version_count(&self) -> usize {
        self.noncurrent_versions.values().map(Vec::len).sum()
    }

    /// Starts a multipart upload.
    pub fn start_multipart_upload(
        &mut self,
        upload_id: String,
        bucket: String,
        key: String,
        storage_class: StorageClass,
        _timestamp: DateTime<Utc>,
    ) {
        self.multipart_uploads.insert(
            upload_id,
            MultipartUploadState {
                bucket,
                key,
                storage_class,
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
        _timestamp: DateTime<Utc>,
    ) -> bool {
        if let Some(upload) = self.multipart_uploads.get_mut(upload_id) {
            upload.parts.insert(part_number, PartState { size });
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
