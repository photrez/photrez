mod cancellation;
mod error;
mod geometry;
mod rate_limit;
mod settings;
mod validation;

// Win32 GDI/spooler calls are unsafe; keep the exception limited to this adapter.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

pub(crate) use cancellation::PrintCancellation;
pub(crate) use error::PrintError;
use rate_limit::PrintRateLimiter;
pub(crate) use settings::{Orientation, PrintSettings};

/// One paper size reported by the selected printer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaperSize {
    pub(crate) name: String,
    pub(crate) index: i16,
    pub(crate) width_mm: f64,
    pub(crate) height_mm: f64,
}

/// A raw RGBA print request. Dimensions and the buffer are validated before
/// any platform adapter receives them.
pub(crate) struct PrintRequest<'a> {
    pub(crate) pixels: &'a [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) settings: &'a PrintSettings,
    pub(crate) document_name: &'a str,
    pub(crate) output_path: Option<&'a str>,
}

/// Stateful service boundary; keep one instance so all print calls share the
/// same cooldown window.
#[derive(Default)]
pub(crate) struct PrintService {
    rate_limiter: PrintRateLimiter,
}

impl PrintService {
    /// Enumerate installed printers. The print dialog that consumes this lands
    /// with the mounted shell workflow.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn list_printers(&self) -> Result<Vec<String>, PrintError> {
        #[cfg(windows)]
        {
            windows::list_printers()
        }
        #[cfg(not(windows))]
        {
            Err(PrintError::UnsupportedPlatform)
        }
    }

    /// Enumerate the selected printer's paper sizes. The Print dialog's Paper
    /// Size row reads this answer; see `App::refresh_print_papers`.
    pub(crate) fn paper_sizes(&self, printer: &str) -> Result<Vec<PaperSize>, PrintError> {
        if printer.is_empty() || printer.contains('\0') {
            return Err(PrintError::validation("Printer name is invalid"));
        }
        #[cfg(windows)]
        {
            windows::paper_sizes(printer)
        }
        #[cfg(not(windows))]
        {
            let _ = printer;
            Err(PrintError::UnsupportedPlatform)
        }
    }

    pub(crate) fn print_rgba(
        &self,
        request: &PrintRequest<'_>,
        cancellation: &PrintCancellation,
    ) -> Result<(), PrintError> {
        if cancellation.is_cancelled() {
            return Err(PrintError::Cancelled);
        }

        validation::validate_request(request)?;
        self.rate_limiter.check()?;

        #[cfg(windows)]
        {
            windows::print_rgba(request, cancellation)
        }

        #[cfg(not(windows))]
        {
            let _ = cancellation;
            Err(PrintError::UnsupportedPlatform)
        }
    }
}

/// The platform boundary the shell dispatches a validated print request to.
///
/// `PrintService` is the production implementation; tests inject a recorder or
/// a failing backend without requiring a printer.
pub(crate) trait PrintDispatch: Send + Sync {
    fn print(
        &self,
        request: &PrintRequest<'_>,
        cancellation: &PrintCancellation,
    ) -> Result<(), PrintError>;
}

impl PrintDispatch for PrintService {
    fn print(
        &self,
        request: &PrintRequest<'_>,
        cancellation: &PrintCancellation,
    ) -> Result<(), PrintError> {
        self.print_rgba(request, cancellation)
    }
}

#[cfg(test)]
mod tests {
    use super::{PrintCancellation, PrintError, PrintRequest, PrintService, PrintSettings};

    #[test]
    fn pre_cancelled_request_returns_without_platform_dispatch() {
        let settings = PrintSettings::default();
        let request = PrintRequest {
            pixels: &[],
            width: 0,
            height: 0,
            settings: &settings,
            document_name: "test",
            output_path: None,
        };
        let cancellation = PrintCancellation::default();
        cancellation.cancel();

        let result = PrintService::default().print_rgba(&request, &cancellation);
        assert!(matches!(result, Err(PrintError::Cancelled)));
    }

    #[test]
    fn repeated_oversized_request_keeps_resource_limit_error() {
        let settings = PrintSettings::default();
        let request = PrintRequest {
            pixels: &[],
            width: 9_000,
            height: 8_000,
            settings: &settings,
            document_name: "test",
            output_path: None,
        };
        let service = PrintService::default();
        let cancellation = PrintCancellation::default();

        let first = service
            .print_rgba(&request, &cancellation)
            .expect_err("the supported document size is above the print buffer limit");
        let second = service
            .print_rgba(&request, &cancellation)
            .expect_err("an invalid request must not be hidden by the cooldown");

        assert_eq!(first.code(), "E_RESOURCE_LIMIT");
        assert_eq!(second.code(), first.code());
    }

    #[cfg(not(windows))]
    #[test]
    fn valid_request_returns_typed_unsupported_platform_error() {
        let pixels = [0, 0, 0, 255];
        let settings = PrintSettings::default();
        let request = PrintRequest {
            pixels: &pixels,
            width: 1,
            height: 1,
            settings: &settings,
            document_name: "test",
            output_path: None,
        };

        let result = PrintService::default().print_rgba(&request, &PrintCancellation::default());
        assert!(matches!(result, Err(PrintError::UnsupportedPlatform)));
    }

    #[cfg(not(windows))]
    #[test]
    fn invalid_request_is_rejected_before_platform_dispatch() {
        let settings = PrintSettings::default();
        let request = PrintRequest {
            pixels: &[],
            width: 1,
            height: 1,
            settings: &settings,
            document_name: "test",
            output_path: None,
        };

        let result = PrintService::default().print_rgba(&request, &PrintCancellation::default());
        assert!(matches!(result, Err(PrintError::Service(_))));
    }

    /// Printer/paper enumeration is a Windows platform capability; the
    /// hardware-gated test is run explicitly on a machine with a printer.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires an installed printer"]
    fn windows_enumerates_printers_and_paper_sizes() {
        let service = PrintService::default();
        let printers = service.list_printers().expect("printers enumerate");
        assert!(!printers.is_empty(), "at least one printer is installed");
        let printer = printers
            .iter()
            .find(|name| name.as_str() == "Microsoft Print to PDF")
            .unwrap_or(&printers[0]);
        let papers = service.paper_sizes(printer).expect("paper sizes enumerate");
        assert!(!papers.is_empty(), "the printer reports paper sizes");
        assert!(
            papers
                .iter()
                .all(|paper| paper.width_mm > 0.0 && paper.height_mm > 0.0),
            "paper dimensions are positive"
        );
    }

    #[test]
    fn invalid_printer_names_are_typed_validation_errors() {
        for printer in ["", "bad\0name"] {
            let error = PrintService::default()
                .paper_sizes(printer)
                .expect_err("invalid printer name is rejected");
            assert_eq!(error.code(), "E_VALIDATION", "{printer:?}");
        }
    }

    /// A missing selected printer must surface `E_PRINTER`, not panic or
    /// silently fall back to a different device.
    #[cfg(windows)]
    #[test]
    fn missing_printer_returns_a_typed_error() {
        let mut settings = PrintSettings::default();
        settings.set_selected_printer(Some("Photrez Nonexistent Printer 12345".to_owned()));
        let pixels = [0u8, 0, 0, 255];
        let request = PrintRequest {
            pixels: &pixels,
            width: 1,
            height: 1,
            settings: &settings,
            document_name: "test",
            output_path: None,
        };
        let error = PrintService::default()
            .print_rgba(&request, &PrintCancellation::default())
            .expect_err("a nonexistent printer is rejected");
        assert_eq!(error.code(), "E_PRINTER", "{error}");
    }

    /// Windows output proof: a real GDI print job to the Microsoft Print to
    /// PDF driver must write a PDF artifact. Ignored by default because it
    /// requires that printer; run explicitly with `--ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires the Microsoft Print to PDF printer"]
    fn windows_print_to_pdf_writes_a_pdf_artifact() {
        use std::time::{Duration, Instant};

        let mut settings = PrintSettings::default();
        settings.set_selected_printer(Some("Microsoft Print to PDF".to_owned()));
        let pixels = [255u8, 0, 0, 255].repeat(16);
        let output =
            std::env::temp_dir().join(format!("photrez-print-proof-{}.pdf", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let output_text = output.to_str().expect("temp path is UTF-8");
        let request = PrintRequest {
            pixels: &pixels,
            width: 4,
            height: 4,
            settings: &settings,
            document_name: "Photrez print proof",
            output_path: Some(output_text),
        };

        PrintService::default()
            .print_rgba(&request, &PrintCancellation::default())
            .expect("the PDF driver accepts the job");

        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if output.is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let bytes = std::fs::read(&output).expect("the driver wrote the output file");
        assert!(
            bytes.starts_with(b"%PDF-"),
            "output is not a PDF: {:?}",
            &bytes[..bytes.len().min(16)]
        );
        eprintln!("print proof: {} ({} bytes)", output.display(), bytes.len());
    }
}
