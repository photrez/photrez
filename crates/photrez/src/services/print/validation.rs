use super::{PrintError, PrintRequest};

const MAX_PRINT_BUFFER_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn validate_request(request: &PrintRequest<'_>) -> Result<(), PrintError> {
    if request.width == 0 || request.height == 0 {
        return Err(PrintError::validation(
            "Print image dimensions must be positive",
        ));
    }

    let expected = u64::from(request.width)
        .checked_mul(u64::from(request.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| PrintError::resource_limit("Print buffer size overflow"))?;
    if expected > MAX_PRINT_BUFFER_BYTES {
        return Err(PrintError::resource_limit(
            "Print data exceeds the 256 MiB limit",
        ));
    }
    let actual = u64::try_from(request.pixels.len())
        .map_err(|_| PrintError::resource_limit("Print buffer size overflow"))?;
    if actual != expected {
        return Err(PrintError::validation(
            "Print data size does not match width x height x 4",
        ));
    }

    request.settings.validate()?;
    if request.document_name.contains('\0')
        || request.output_path.is_some_and(|path| path.contains('\0'))
    {
        return Err(PrintError::validation(
            "Print metadata contains a null character",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_request, MAX_PRINT_BUFFER_BYTES};
    use crate::services::print::{PrintError, PrintRequest, PrintSettings};

    fn request<'a>(
        pixels: &'a [u8],
        width: u32,
        height: u32,
        settings: &'a PrintSettings,
    ) -> PrintRequest<'a> {
        PrintRequest {
            pixels,
            width,
            height,
            settings,
            document_name: "test",
            output_path: None,
        }
    }

    #[test]
    fn exact_rgba_buffer_is_accepted() {
        let settings = PrintSettings::default();
        let pixels = [0; 16];
        assert!(validate_request(&request(&pixels, 2, 2, &settings)).is_ok());
    }

    #[test]
    fn mismatched_and_zero_dimension_buffers_are_rejected() {
        let settings = PrintSettings::default();
        assert!(matches!(
            validate_request(&request(&[0; 15], 2, 2, &settings)),
            Err(PrintError::Service(_))
        ));
        assert!(matches!(
            validate_request(&request(&[0; 17], 2, 2, &settings)),
            Err(PrintError::Service(_))
        ));
        assert!(matches!(
            validate_request(&request(&[], 0, 1, &settings)),
            Err(PrintError::Service(_))
        ));
    }

    #[test]
    fn oversized_and_overflowing_dimensions_are_resource_limited() {
        let settings = PrintSettings::default();
        const { assert!(MAX_PRINT_BUFFER_BYTES >= 8000 * 8000 * 4) };
        assert!(matches!(
            validate_request(&request(&[], 9000, 8000, &settings)),
            Err(PrintError::Service(_))
        ));
        assert!(matches!(
            validate_request(&request(&[], u32::MAX, u32::MAX, &settings)),
            Err(PrintError::Service(_))
        ));
    }
}
