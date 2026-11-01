use crate::services::error::ServiceError;

pub const MAX_DOCUMENT_DECODED_RGBA_BYTES: u64 = 1_073_741_824;
pub const MAX_LAYER_DIMENSION: u64 = 16_384;
pub const MAX_LAYER_COUNT: usize = 200;
pub const MAX_STRUCTURAL_ENTRIES: usize = 201;
pub const MAX_DOCUMENT_JSON_BYTES: u64 = 1_048_576;
pub const MAX_ARCHIVE_ENTRY_BYTES: u64 = 2_147_483_648;
/// Imported image bytes stay under the pinned reference's file boundary.
pub const MAX_IMAGE_FILE_BYTES: u64 = 256 * 1024 * 1024;

pub fn validate_layer_id(layer_id: &str) -> Result<(), ServiceError> {
    let bytes = layer_id.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 128
        || layer_id == "."
        || layer_id == ".."
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ServiceError::Validation(
            "layer ID must be 1 to 128 ASCII letters, digits, dots, underscores, or hyphens and not a path component".into(),
        ));
    }
    Ok(())
}

pub fn decoded_rgba_bytes(width: u64, height: u64) -> Result<u64, ServiceError> {
    let bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| ServiceError::ResourceLimit("decoded RGBA size overflow".into()))?;
    validate_dimensions(width, height)?;
    if bytes > MAX_DOCUMENT_DECODED_RGBA_BYTES {
        return Err(ServiceError::ResourceLimit(
            "decoded RGBA layer exceeds the per-document budget".into(),
        ));
    }
    Ok(bytes)
}

fn validate_dimensions(width: u64, height: u64) -> Result<(), ServiceError> {
    if width == 0 || height == 0 || width > MAX_LAYER_DIMENSION || height > MAX_LAYER_DIMENSION {
        return Err(ServiceError::Validation(
            "layer dimensions must be between 1 and 16384 pixels".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct ArchiveBudget {
    document_json_reserved: bool,
    total_decoded_bytes: u64,
    total_encoded_bytes: u64,
    layer_count: usize,
    entry_count: usize,
}

impl ArchiveBudget {
    pub const fn new() -> Self {
        Self {
            document_json_reserved: false,
            total_decoded_bytes: 0,
            total_encoded_bytes: 0,
            layer_count: 0,
            entry_count: 0,
        }
    }

    pub fn reserve_document_json(&mut self, bytes: u64) -> Result<(), ServiceError> {
        if bytes > MAX_DOCUMENT_JSON_BYTES {
            return Err(ServiceError::ResourceLimit(
                "document.json exceeds its metadata size limit".into(),
            ));
        }
        if self.document_json_reserved {
            return Err(ServiceError::Validation(
                "document.json has already been reserved".into(),
            ));
        }
        let entry_count = self
            .entry_count
            .checked_add(1)
            .filter(|count| *count <= MAX_STRUCTURAL_ENTRIES)
            .ok_or_else(|| ServiceError::ResourceLimit("archive entry limit exceeded".into()))?;
        let total_encoded_bytes = self
            .total_encoded_bytes
            .checked_add(bytes)
            .filter(|total| *total <= MAX_ARCHIVE_ENTRY_BYTES)
            .ok_or_else(|| {
                ServiceError::ResourceLimit("archive entry byte limit exceeded".into())
            })?;
        self.document_json_reserved = true;
        self.entry_count = entry_count;
        self.total_encoded_bytes = total_encoded_bytes;
        Ok(())
    }

    pub fn reserve_layer(
        &mut self,
        layer_id: &str,
        width: u64,
        height: u64,
        encoded_bytes: u64,
    ) -> Result<(), ServiceError> {
        validate_layer_id(layer_id)?;
        let decoded_bytes = decoded_rgba_bytes(width, height)?;
        if !self.document_json_reserved {
            return Err(ServiceError::Validation(
                "document.json must be reserved before layers".into(),
            ));
        }
        let layer_count = self
            .layer_count
            .checked_add(1)
            .filter(|count| *count <= MAX_LAYER_COUNT)
            .ok_or_else(|| ServiceError::ResourceLimit("archive layer limit exceeded".into()))?;
        let entry_count = self
            .entry_count
            .checked_add(1)
            .filter(|count| *count <= MAX_STRUCTURAL_ENTRIES)
            .ok_or_else(|| ServiceError::ResourceLimit("archive entry limit exceeded".into()))?;
        let total_decoded_bytes = self
            .total_decoded_bytes
            .checked_add(decoded_bytes)
            .filter(|total| *total <= MAX_DOCUMENT_DECODED_RGBA_BYTES)
            .ok_or_else(|| {
                ServiceError::ResourceLimit("per-document decoded RGBA budget exceeded".into())
            })?;
        let total_encoded_bytes = self
            .total_encoded_bytes
            .checked_add(encoded_bytes)
            .filter(|total| *total <= MAX_ARCHIVE_ENTRY_BYTES)
            .ok_or_else(|| {
                ServiceError::ResourceLimit("archive entry byte limit exceeded".into())
            })?;
        self.total_decoded_bytes = total_decoded_bytes;
        self.total_encoded_bytes = total_encoded_bytes;
        self.layer_count = layer_count;
        self.entry_count = entry_count;
        Ok(())
    }

    pub const fn total_decoded_bytes(&self) -> u64 {
        self.total_decoded_bytes
    }

    pub const fn total_encoded_bytes(&self) -> u64 {
        self.total_encoded_bytes
    }

    pub const fn layer_count(&self) -> usize {
        self.layer_count
    }

    pub const fn entry_count(&self) -> usize {
        self.entry_count
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decoded_rgba_bytes, validate_layer_id, ArchiveBudget, MAX_ARCHIVE_ENTRY_BYTES,
        MAX_DOCUMENT_DECODED_RGBA_BYTES, MAX_DOCUMENT_JSON_BYTES, MAX_LAYER_COUNT,
        MAX_LAYER_DIMENSION, MAX_STRUCTURAL_ENTRIES,
    };
    use crate::services::error::ServiceError;

    #[test]
    fn layer_ids_accept_ascii_boundaries_and_reject_invalid_components() {
        for id in ["A", "a", "0", "name.part", "_", "-"] {
            assert!(validate_layer_id(id).is_ok(), "{id:?}");
        }
        assert!(validate_layer_id(&"a".repeat(128)).is_ok());

        for id in ["", "a/b", "a\\b", "a\0b", "a\nb", "é", ".", ".."] {
            assert!(
                matches!(validate_layer_id(id), Err(ServiceError::Validation(_))),
                "{id:?}"
            );
        }
        assert!(matches!(
            validate_layer_id(&"a".repeat(129)),
            Err(ServiceError::Validation(_))
        ));
    }

    #[test]
    fn decoded_rgba_budget_is_exactly_one_gibibyte_per_document() {
        assert_eq!(MAX_DOCUMENT_DECODED_RGBA_BYTES, 1_073_741_824);
        assert_eq!(MAX_LAYER_DIMENSION, 16_384);
        assert_eq!(
            decoded_rgba_bytes(16_384, 16_384).unwrap(),
            MAX_DOCUMENT_DECODED_RGBA_BYTES
        );
    }

    #[test]
    fn malformed_dimensions_and_arithmetic_overflow_are_refused() {
        for (width, height) in [(0, 1), (1, 0), (16_385, 1), (1, 16_385)] {
            assert!(matches!(
                decoded_rgba_bytes(width, height),
                Err(ServiceError::Validation(_))
            ));
        }
        assert!(matches!(
            decoded_rgba_bytes(u64::MAX, 1),
            Err(ServiceError::ResourceLimit(_))
        ));
    }

    #[test]
    fn archive_budget_checks_document_bytes_and_decoded_bytes() {
        assert_eq!(MAX_DOCUMENT_JSON_BYTES, 1_048_576);
        let mut budget = ArchiveBudget::new();
        assert!(budget
            .reserve_document_json(MAX_DOCUMENT_JSON_BYTES)
            .is_ok());
        assert_eq!(budget.entry_count(), 1);
        assert_eq!(budget.total_encoded_bytes(), MAX_DOCUMENT_JSON_BYTES);

        let mut oversized_metadata = ArchiveBudget::new();
        assert!(matches!(
            oversized_metadata.reserve_document_json(MAX_DOCUMENT_JSON_BYTES + 1),
            Err(ServiceError::ResourceLimit(_))
        ));
        assert_eq!(oversized_metadata.entry_count(), 0);
        assert_eq!(oversized_metadata.total_encoded_bytes(), 0);

        let mut decoded_budget = ArchiveBudget::new();
        decoded_budget.reserve_document_json(0).unwrap();
        decoded_budget
            .reserve_layer("full", 16_384, 16_384, 0)
            .unwrap();
        assert_eq!(
            decoded_budget.total_decoded_bytes(),
            MAX_DOCUMENT_DECODED_RGBA_BYTES
        );
        assert!(matches!(
            decoded_budget.reserve_layer("extra", 1, 1, 0),
            Err(ServiceError::ResourceLimit(_))
        ));
        assert_eq!(decoded_budget.layer_count(), 1);
    }

    #[test]
    fn archive_budget_allows_200_layers_and_201_structural_entries() {
        assert_eq!(MAX_LAYER_COUNT, 200);
        assert_eq!(MAX_STRUCTURAL_ENTRIES, 201);
        let mut budget = ArchiveBudget::new();
        budget.reserve_document_json(1).unwrap();
        for index in 0..MAX_LAYER_COUNT {
            budget
                .reserve_layer(&format!("layer-{index}"), 1, 1, 1)
                .unwrap();
        }

        assert_eq!(budget.layer_count(), 200);
        assert_eq!(budget.entry_count(), MAX_STRUCTURAL_ENTRIES);
        assert_eq!(budget.total_decoded_bytes(), 800);
        assert_eq!(budget.total_encoded_bytes(), 201);
        assert!(matches!(
            budget.reserve_layer("overflow", 1, 1, 1),
            Err(ServiceError::ResourceLimit(_))
        ));
        assert_eq!(budget.layer_count(), 200);
        assert_eq!(budget.entry_count(), MAX_STRUCTURAL_ENTRIES);
    }

    #[test]
    fn layer_accounting_accepts_the_supplied_encoded_length() {
        let mut budget = ArchiveBudget::new();
        budget.reserve_document_json(0).unwrap();
        budget.reserve_layer("layer", 1, 1, 88).unwrap();
        assert_eq!(budget.total_encoded_bytes(), 88);
    }

    #[test]
    fn aggregate_encoded_budget_accepts_exact_limit_including_metadata() {
        const AGGREGATE_LIMIT: u64 = 2_147_483_648;
        const DOCUMENT_BYTES: u64 = 9;
        assert_eq!(MAX_ARCHIVE_ENTRY_BYTES, AGGREGATE_LIMIT);
        let mut budget = ArchiveBudget::new();
        budget.reserve_document_json(DOCUMENT_BYTES).unwrap();
        budget
            .reserve_layer("at-limit", 1, 1, AGGREGATE_LIMIT - DOCUMENT_BYTES)
            .unwrap();

        assert_eq!(budget.total_encoded_bytes(), AGGREGATE_LIMIT);
    }

    #[test]
    fn aggregate_refusal_preserves_counters_and_allows_a_later_reservation() {
        const AGGREGATE_LIMIT: u64 = 2_147_483_648;
        const DOCUMENT_BYTES: u64 = 17;
        let mut budget = ArchiveBudget::new();
        budget.reserve_document_json(DOCUMENT_BYTES).unwrap();

        assert!(matches!(
            budget.reserve_layer("too-large", 1, 1, AGGREGATE_LIMIT - DOCUMENT_BYTES + 1),
            Err(ServiceError::ResourceLimit(_))
        ));
        assert_eq!(budget.total_decoded_bytes(), 0);
        assert_eq!(budget.total_encoded_bytes(), DOCUMENT_BYTES);
        assert_eq!(budget.layer_count(), 0);
        assert_eq!(budget.entry_count(), 1);

        budget.reserve_layer("retry", 1, 1, 1).unwrap();
        assert_eq!(budget.total_decoded_bytes(), 4);
        assert_eq!(budget.total_encoded_bytes(), DOCUMENT_BYTES + 1);
        assert_eq!(budget.layer_count(), 1);
        assert_eq!(budget.entry_count(), 2);
    }
}
