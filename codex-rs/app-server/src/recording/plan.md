# Cross-Platform Screen Recording Service in `codex-app-server`

## Summary

- Add a process-scoped, in-process `ScreenRecordingManager` to `codex-app-server`, owned by `MessageProcessor` rather than a thread.
- Add `features.screen_recording` as the coarse rollout gate, and keep `recording.screen.enabled` as the persistent user opt-in bit.
- If app-server starts with both `features.screen_recording = true` and `recording.screen.enabled = true`, it begins recording automatically without any extra app-server command.
- Keep v1 capture-only: capture all active displays at `1 fps`, write growing per-segment MP4 files under `$CODEX_HOME/recording/screen_ephemeral`, prune anything older than `6 hours`.
- Keep `recording/screen/pause` and `recording/screen/resume` as runtime-only controls. They temporarily stop or resume capture but do not change `recording.screen.enabled`.
- Use one app-server API across macOS, Windows, and Linux; unsupported or unpermissioned hosts must report explicit status instead of silently degrading.

## Key Changes
- Add a typed `[recording.screen]` config block in core/app-server config with `enabled`, and expose it through `config/read` plus config schema updates.
- Add `ScreenRecordingManager` under a new `app-server` module (for example `app-server/src/recording/`) with its own lifecycle, reconcile it at startup, on relevant config writes, and during app-server shutdown.
- Start capture automatically when the effective config has both `features.screen_recording = true` and `recording.screen.enabled = true`.
- Stop capture and purge local artifacts when either the feature gate flips to `false` or `recording.screen.enabled` flips to `false`.
- Do not rewrite `recording.screen.enabled` when the feature gate turns off; preserve the user opt-in so capture resumes automatically if the feature is later re-enabled.
- Use `xcap` to do screen capture.
- Add a concrete in-process `FfmpegSegmentEncoder` for v1 rather than a `SegmentEncoder` trait. Use FFmpeg libraries through Rust bindings, not an external `ffmpeg` binary.
- Capture each display separately once per second, offload grab/encode/write work off the request path, append frames into a growing fragmented MP4 segment, write a timestamped manifest alongside it, and prune on startup and continuously.
- Do not introduce encoder trait indirection in v1. Keep the encoder concrete until there is a second production implementation or a clear test seam that needs it.
- Track displays by stable backend/OS display ID rather than enumeration order so hotplug, reordering, and dock/undock events do not accidentally merge or swap monitor streams.
- Keep privacy boundaries hard in v1: no audio, no keylogging, no content redaction, and no RPC that returns frame content.

## Things to get right

- Multi-monitor hotplug behavior. Displays are tracked independently, new monitors create streams, and disconnected monitors are only closed after 3 missed ticks to avoid reacting to a brief transient miss as a real unplug.
- Monitor resolution changes mid-recording. A stream rotates to a new MP4 segment when width/height changes, which avoids feeding one encoder context frames with incompatible dimensions.
- Rolling segments. The writer keeps one active fragmented MP4 per display segment and rotates every 30 minutes, with additional rotation when display geometry changes or the date directory changes at midnight.
- Crash/restart behavior. Do not reopen an old MP4 for append after restart; start a new segment and rely on fragmented MP4 so the in-progress file is still useful if the process dies before a clean close.

## Public APIs / Types
- Add experimental app-server requests:
  - `recording/screen/read`
  - `recording/screen/pause`
  - `recording/screen/resume`
- `recording/screen/read` should still return status when the feature is off so clients can see that recording is gated.
- `recording/screen/pause` and `recording/screen/resume` should reject while `features.screen_recording = false`.
- Do not add `recording/screen/enable` or `recording/screen/disable`. Persistent enablement is handled through existing config write APIs by changing `recording.screen.enabled`.
- Add experimental notification `recording/screen/status/updated`, broadcast to initialized clients when service state changes.
- Add typed models:
  - `ScreenRecordingConfig` with `enabled`
  - `ScreenRecordingStatus` with `state`, `paused`, `platform`, `backend`, `permission`, `capture_fps`, `retention_hours`, `storage_path`, `captured_display_count`, `newest_frame_at`, and `last_error`
- Expose `recording` in `config/read` behind experimental field gating until the feature exits internal rollout.
- Regenerate app-server schemas and update `codex-rs/app-server/README.md` plus `codex-rs/core/config.schema.json`.

## Test Plan
- Protocol tests for request/notification serialization and experimental gating.
- Config tests for parsing, defaults, schema generation, and `config/read` exposure of `[recording.screen]`.
- Manager unit tests with a fake capture backend covering disabled startup, enabled startup auto-begins capture, pause/resume, permission-denied, runtime backend failure, prune-after-6h, disable-via-config stops and purges, and shutdown drain.
- App-server integration tests covering startup autostart from combined config + feature gate, `recording/screen/read`, `pause`/`resume`, `recording/screen/status/updated`, feature-flag reconciliation, config-write reconciliation, and notification opt-out behavior.
- Encoder tests should verify that a live segment file is created, grows as frames are appended, and is rotated on geometry/date/time-boundary changes.
- Platform smoke tests that compile each backend on its target OS; CI runtime tests use the fake capture backend rather than real capture permissions.

## Assumptions
- The service is process-scoped, not thread-scoped.
- `paused` is runtime-only and does not persist across restart.
- Avoiding an external encoder binary is a hard requirement for v1, even if that means taking on FFmpeg library packaging/linking complexity in-process.
- Future quick-reference and memory-generation work will consume the manifest/artifact layout added here, but that pipeline is intentionally out of scope for this first implementation.
