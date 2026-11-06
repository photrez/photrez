# Frame-pacing measurements

Frame pacing is measured in the running application:
`cargo run -p photrez --bin measure-frames` records dispatch, composite and
presentation timings for a synthetic stroke, and `measure-input` records the
first-pixel and preview-push latencies the responsiveness budget is stated
against. Every row either one prints names the build profile it ran under, `dev`
or `release`, so a number is never quoted without the fact that decides whether
it is comparable. The frame rows also record the harness, mode, canvas size,
layer count and stroke counts they measured, in milliseconds.
