# Deterministic CI separated from live YouTube integration checks

yt-lite will use vertical-slice development with automated unit and integration tests based on deterministic fixtures and mocked services for routine pull-request checks. Real Windows playback tests and separately triggered or scheduled YouTube integration tests will verify behavior that cannot be established offline, without making every pull request depend on upstream network availability.

**Consequences:** The live test suite is still necessary to detect extractor regressions; results should be reported distinctly from deterministic CI. Startup, idle resource use, scrolling, and playback frame pacing must be measured against established baselines on both development and lower-end hardware. Diagnostic logs remain local and exportable with actionable errors; no telemetry will be sent. The first promoted public release targets daily-driver beta quality, not complete delivery of all advanced features.
