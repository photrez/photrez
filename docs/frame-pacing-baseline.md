# Frame-pacing measurements

The dated measurement record this file pointed to described a runtime this
product no longer uses and is not part of this repository.

Frame pacing is measured in the running application instead:
`cargo run -p photrez --bin measure-frames` records dispatch, composite and
presentation timings for a synthetic stroke, and `measure-input` records the
first-pixel and preview-push latencies the responsiveness budget is stated
against. Both report the build and profile they ran on.
