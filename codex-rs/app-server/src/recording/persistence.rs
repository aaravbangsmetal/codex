use super::backend::CaptureBackend;
use super::backend::CaptureBackendFailure;
use super::backend::CapturedDisplay;
use super::backend::DisplayGeometry;
use chrono::DateTime;
use chrono::Local;
use chrono::Utc;
use image::ColorType;
use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;

pub(crate) const CAPTURE_FPS: u32 = 1;
pub(crate) const RETENTION_HOURS: u32 = 6;
pub(crate) const RETENTION_SECONDS: i64 = 6 * 60 * 60;
pub(crate) const SEGMENT_LENGTH_SECONDS: i64 = 30 * 60;
pub(crate) const DISPLAY_REMOVAL_MISSED_TICKS: u32 = 3;
const MANIFEST_FILE_NAME: &str = "manifest.json";

#[derive(Debug, Default)]
pub(crate) struct CaptureState {
    display_streams: HashMap<String, DisplayStream>,
    known_display_ids: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CaptureTickOutcome {
    pub(crate) captured_display_count: u32,
    pub(crate) newest_frame_at: Option<i64>,
}

#[derive(Debug)]
struct DisplayStream {
    name: String,
    geometry: DisplayGeometry,
    missed_ticks: u32,
    segment: SegmentState,
}

#[derive(Debug)]
struct SegmentState {
    directory: PathBuf,
    bucket_start_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SegmentReason {
    Started,
    Hotplugged,
    GeometryChanged,
    TimeBucketChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SegmentManifest {
    version: u32,
    display_id: String,
    display_name: String,
    width: u32,
    height: u32,
    rotation_millidegrees: i32,
    scale_factor_milli: u32,
    segment_started_at: i64,
    segment_reason: SegmentReason,
    frame_count: u64,
    newest_frame_at: Option<i64>,
}

pub(crate) fn capture_tick(
    storage_root: &Path,
    state: &mut CaptureState,
    backend: &dyn CaptureBackend,
    captured_at: DateTime<Utc>,
) -> Result<CaptureTickOutcome, CaptureBackendFailure> {
    let displays = backend.capture_displays()?;
    fs::create_dir_all(storage_root)
        .map_err(|err| CaptureBackendFailure::other(err.to_string()))?;

    let mut newest_frame_at = None;
    let seen_ids: HashSet<String> = displays.iter().map(|display| display.id.clone()).collect();

    for display in displays {
        let stream = upsert_display_stream(storage_root, state, &display, captured_at)
            .map_err(|err| CaptureBackendFailure::other(err.to_string()))?;
        write_frame(storage_root, stream, &display, captured_at)
            .map_err(|err| CaptureBackendFailure::other(err.to_string()))?;
        newest_frame_at = Some(
            newest_frame_at
                .unwrap_or(captured_at.timestamp())
                .max(captured_at.timestamp()),
        );
    }

    let missing_ids = state
        .display_streams
        .keys()
        .filter(|id| !seen_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in missing_ids {
        if let Some(stream) = state.display_streams.get_mut(&id) {
            stream.missed_ticks = stream.missed_ticks.saturating_add(1);
            if stream.missed_ticks >= DISPLAY_REMOVAL_MISSED_TICKS {
                state.display_streams.remove(&id);
            }
        }
    }

    Ok(CaptureTickOutcome {
        captured_display_count: state
            .display_streams
            .values()
            .filter(|stream| stream.missed_ticks == 0)
            .count() as u32,
        newest_frame_at,
    })
}

pub(crate) fn prune_old_segments(
    storage_root: &Path,
    captured_at: DateTime<Utc>,
) -> std::io::Result<()> {
    if !storage_root.exists() {
        return Ok(());
    }

    let cutoff = captured_at.timestamp() - RETENTION_SECONDS;
    let manifest_paths = WalkDir::new(storage_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == MANIFEST_FILE_NAME)
        .map(walkdir::DirEntry::into_path)
        .collect::<Vec<_>>();

    for manifest_path in manifest_paths {
        let Ok(bytes) = fs::read(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<SegmentManifest>(&bytes) else {
            continue;
        };
        let newest_frame_at = manifest
            .newest_frame_at
            .unwrap_or(manifest.segment_started_at);
        if newest_frame_at < cutoff
            && let Some(segment_dir) = manifest_path.parent()
        {
            let _ = fs::remove_dir_all(segment_dir);
        }
    }

    let directories = WalkDir::new(storage_root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(walkdir::DirEntry::into_path)
        .collect::<Vec<_>>();

    for directory in directories.into_iter().rev() {
        if directory.read_dir()?.next().is_none() {
            let _ = fs::remove_dir(&directory);
        }
    }

    Ok(())
}

pub(crate) fn purge_storage(storage_root: &Path) -> std::io::Result<()> {
    if storage_root.exists() {
        fs::remove_dir_all(storage_root)?;
    }
    Ok(())
}

fn upsert_display_stream<'a>(
    storage_root: &Path,
    state: &'a mut CaptureState,
    display: &CapturedDisplay,
    captured_at: DateTime<Utc>,
) -> std::io::Result<&'a mut DisplayStream> {
    let bucket_start_at = bucket_start_at(captured_at);
    let is_known_display = state.known_display_ids.contains(&display.id);

    let rotate_reason = if let Some(stream) = state.display_streams.get(&display.id) {
        if stream.geometry != display.geometry {
            Some(SegmentReason::GeometryChanged)
        } else if stream.segment.bucket_start_at != bucket_start_at {
            Some(SegmentReason::TimeBucketChanged)
        } else {
            None
        }
    } else if is_known_display {
        Some(SegmentReason::Hotplugged)
    } else {
        Some(SegmentReason::Started)
    };

    if let Some(stream) = state.display_streams.get_mut(&display.id) {
        stream.name = display.name.clone();
        stream.geometry = display.geometry;
        stream.missed_ticks = 0;
        if let Some(reason) = rotate_reason {
            stream.segment = open_segment(
                storage_root,
                &display.id,
                &display.name,
                display.geometry,
                captured_at,
                reason,
            )?;
        }
    } else {
        state.known_display_ids.insert(display.id.clone());
        let segment = open_segment(
            storage_root,
            &display.id,
            &display.name,
            display.geometry,
            captured_at,
            rotate_reason.unwrap_or(SegmentReason::Started),
        )?;
        state.display_streams.insert(
            display.id.clone(),
            DisplayStream {
                name: display.name.clone(),
                geometry: display.geometry,
                missed_ticks: 0,
                segment,
            },
        );
    }

    match state.display_streams.get_mut(&display.id) {
        Some(stream) => Ok(stream),
        None => Err(std::io::Error::other(
            "display stream should exist after upsert",
        )),
    }
}

fn open_segment(
    storage_root: &Path,
    display_id: &str,
    display_name: &str,
    geometry: DisplayGeometry,
    captured_at: DateTime<Utc>,
    reason: SegmentReason,
) -> std::io::Result<SegmentState> {
    let local = captured_at.with_timezone(&Local);
    let bucket_start_at = bucket_start_at(captured_at);
    let segment_dir = storage_root.join(format!(
        "{}-display-{display_id}-{}",
        local.format("%Y-%m-%dT%H-%M-%S"),
        segment_reason_name(reason)
    ));
    fs::create_dir_all(&segment_dir)?;
    let manifest = SegmentManifest {
        version: 1,
        display_id: display_id.to_string(),
        display_name: display_name.to_string(),
        width: geometry.width,
        height: geometry.height,
        rotation_millidegrees: geometry.rotation_millidegrees,
        scale_factor_milli: geometry.scale_factor_milli,
        segment_started_at: captured_at.timestamp(),
        segment_reason: reason,
        frame_count: 0,
        newest_frame_at: None,
    };
    write_manifest(&segment_dir, &manifest)?;
    Ok(SegmentState {
        directory: segment_dir,
        bucket_start_at,
    })
}

fn write_frame(
    _storage_root: &Path,
    stream: &DisplayStream,
    display: &CapturedDisplay,
    captured_at: DateTime<Utc>,
) -> std::io::Result<()> {
    let frame_path = stream
        .segment
        .directory
        .join(format!("frame-{}.jpg", captured_at.timestamp_millis()));
    let file = File::create(&frame_path)?;
    let mut writer = BufWriter::new(file);
    let rgb_frame = image::DynamicImage::ImageRgba8(display.frame.clone()).into_rgb8();
    let encoder = JpegEncoder::new_with_quality(&mut writer, 85);
    encoder
        .write_image(
            rgb_frame.as_raw(),
            rgb_frame.width(),
            rgb_frame.height(),
            ColorType::Rgb8.into(),
        )
        .map_err(std::io::Error::other)?;

    let manifest_path = stream.segment.directory.join(MANIFEST_FILE_NAME);
    let manifest_bytes = fs::read(&manifest_path)?;
    let mut manifest: SegmentManifest =
        serde_json::from_slice(&manifest_bytes).map_err(std::io::Error::other)?;
    manifest.frame_count = manifest.frame_count.saturating_add(1);
    manifest.newest_frame_at = Some(captured_at.timestamp());
    write_manifest(&stream.segment.directory, &manifest)
}

fn write_manifest(segment_dir: &Path, manifest: &SegmentManifest) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(manifest).map_err(std::io::Error::other)?;
    fs::write(segment_dir.join(MANIFEST_FILE_NAME), bytes)
}

fn bucket_start_at(captured_at: DateTime<Utc>) -> i64 {
    let timestamp = captured_at.timestamp();
    timestamp - (timestamp % SEGMENT_LENGTH_SECONDS)
}

fn segment_reason_name(reason: SegmentReason) -> &'static str {
    match reason {
        SegmentReason::Started => "started",
        SegmentReason::Hotplugged => "hotplugged",
        SegmentReason::GeometryChanged => "geometry",
        SegmentReason::TimeBucketChanged => "time-bucket",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::backend::CaptureBackendFailureKind;
    use image::Rgba;
    use image::RgbaImage;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    struct SequenceBackend {
        frames: std::sync::Mutex<Vec<Result<Vec<CapturedDisplay>, CaptureBackendFailure>>>,
    }

    impl SequenceBackend {
        fn new(frames: Vec<Result<Vec<CapturedDisplay>, CaptureBackendFailure>>) -> Self {
            Self {
                frames: std::sync::Mutex::new(frames),
            }
        }
    }

    impl CaptureBackend for SequenceBackend {
        fn kind(&self) -> codex_app_server_protocol::ScreenRecordingBackend {
            codex_app_server_protocol::ScreenRecordingBackend::Xcap
        }

        fn platform(&self) -> codex_app_server_protocol::ScreenRecordingPlatform {
            codex_app_server_protocol::ScreenRecordingPlatform::Macos
        }

        fn capture_displays(&self) -> Result<Vec<CapturedDisplay>, CaptureBackendFailure> {
            self.frames.lock().expect("sequence lock").remove(0)
        }
    }

    fn display(id: &str, width: u32, height: u32) -> CapturedDisplay {
        let mut frame = RgbaImage::new(width, height);
        for pixel in frame.pixels_mut() {
            *pixel = Rgba([10, 20, 30, 255]);
        }
        CapturedDisplay {
            id: id.to_string(),
            name: format!("Display {id}"),
            geometry: DisplayGeometry {
                width,
                height,
                rotation_millidegrees: 0,
                scale_factor_milli: 1000,
            },
            frame,
        }
    }

    #[test]
    fn hotplug_misses_are_debounced_for_three_ticks() {
        let tmp = TempDir::new().expect("tmpdir");
        let backend = SequenceBackend::new(vec![
            Ok(vec![display("1", 64, 48)]),
            Ok(vec![]),
            Ok(vec![]),
            Ok(vec![display("1", 64, 48)]),
        ]);
        let mut state = CaptureState::default();

        for second in 0..4 {
            capture_tick(
                tmp.path(),
                &mut state,
                &backend,
                DateTime::from_timestamp(1_700_000_000 + second, 0).expect("timestamp"),
            )
            .expect("capture tick");
        }

        assert_eq!(state.display_streams.len(), 1);
        assert_eq!(state.display_streams["1"].missed_ticks, 0);
    }

    #[test]
    fn geometry_change_rotates_segments() {
        let tmp = TempDir::new().expect("tmpdir");
        let backend = SequenceBackend::new(vec![
            Ok(vec![display("1", 64, 48)]),
            Ok(vec![display("1", 80, 60)]),
        ]);
        let mut state = CaptureState::default();

        capture_tick(
            tmp.path(),
            &mut state,
            &backend,
            DateTime::from_timestamp(1_700_000_000, 0).expect("timestamp"),
        )
        .expect("first capture");
        capture_tick(
            tmp.path(),
            &mut state,
            &backend,
            DateTime::from_timestamp(1_700_000_001, 0).expect("timestamp"),
        )
        .expect("second capture");

        let segment_count = WalkDir::new(tmp.path())
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir() && entry.depth() == 1)
            .count();
        assert_eq!(segment_count, 2);
    }

    #[test]
    fn segment_directories_are_flat_timestamped_folders() {
        let tmp = TempDir::new().expect("tmpdir");
        let backend = SequenceBackend::new(vec![
            Ok(vec![display("1", 64, 48)]),
            Ok(vec![display("1", 64, 48)]),
        ]);
        let mut state = CaptureState::default();

        capture_tick(
            tmp.path(),
            &mut state,
            &backend,
            DateTime::parse_from_rfc3339("2026-03-23T23:59:59Z")
                .expect("parse")
                .with_timezone(&Utc),
        )
        .expect("first capture");
        capture_tick(
            tmp.path(),
            &mut state,
            &backend,
            DateTime::parse_from_rfc3339("2026-03-24T00:00:01Z")
                .expect("parse")
                .with_timezone(&Utc),
        )
        .expect("second capture");

        let mut segment_dirs = WalkDir::new(tmp.path())
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir() && entry.depth() == 1)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        segment_dirs.sort();

        let first_local = DateTime::parse_from_rfc3339("2026-03-23T23:59:59Z")
            .expect("parse")
            .with_timezone(&Local);
        let second_local = DateTime::parse_from_rfc3339("2026-03-24T00:00:01Z")
            .expect("parse")
            .with_timezone(&Local);
        assert_eq!(
            segment_dirs,
            vec![
                format!(
                    "{}-display-1-started",
                    first_local.format("%Y-%m-%dT%H-%M-%S")
                ),
                format!(
                    "{}-display-1-time-bucket",
                    second_local.format("%Y-%m-%dT%H-%M-%S")
                ),
            ]
        );
    }

    #[test]
    fn prune_removes_old_segments() {
        let tmp = TempDir::new().expect("tmpdir");
        let old_segment = tmp.path().join("2026-03-20T00-00-00-display-1-started");
        fs::create_dir_all(&old_segment).expect("create old segment");
        write_manifest(
            &old_segment,
            &SegmentManifest {
                version: 1,
                display_id: "1".to_string(),
                display_name: "Display 1".to_string(),
                width: 64,
                height: 48,
                rotation_millidegrees: 0,
                scale_factor_milli: 1000,
                segment_started_at: 1_700_000_000,
                segment_reason: SegmentReason::Started,
                frame_count: 1,
                newest_frame_at: Some(1_700_000_000),
            },
        )
        .expect("write manifest");

        prune_old_segments(
            tmp.path(),
            DateTime::from_timestamp(1_700_000_000 + RETENTION_SECONDS + 1, 0).expect("timestamp"),
        )
        .expect("prune");

        assert!(!old_segment.exists());
    }

    #[test]
    fn backend_errors_propagate() {
        let tmp = TempDir::new().expect("tmpdir");
        let backend = SequenceBackend::new(vec![Err(CaptureBackendFailure {
            kind: CaptureBackendFailureKind::PermissionRequired,
            permission: codex_app_server_protocol::ScreenRecordingPermission::Required,
            message: "permission required".to_string(),
        })]);
        let mut state = CaptureState::default();
        let error = capture_tick(
            tmp.path(),
            &mut state,
            &backend,
            DateTime::from_timestamp(1_700_000_000, 0).expect("timestamp"),
        )
        .expect_err("capture should fail");
        assert_eq!(error.message, "permission required");
    }
}
