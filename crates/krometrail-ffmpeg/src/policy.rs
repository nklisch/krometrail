use std::{ffi::OsString, time::Duration};

use krometrail_core::VideoOutputGeometry;

pub const FFMPEG_ARGUMENT_POLICY_VERSION: &str = "krometrail-ffmpeg-h264-v1";
pub const FFMPEG_ENCODER_ALLOWLIST: &[&str] = &["libx264"];
pub const MAX_FFMPEG_VERSION_REPORT_BYTES: usize = 64 * 1024;
pub const MAX_FFMPEG_STDERR_BYTES: usize = 64 * 1024;
pub const MAX_FFMPEG_DISCOVERY_CANDIDATES: usize = 16;
pub const FFMPEG_QUALIFICATION_TIMEOUT: Duration = Duration::from_secs(5);
pub const FFMPEG_TERMINATION_GRACE: Duration = Duration::from_millis(250);
pub const FFMPEG_TIMEBASE_HZ: u32 = 1_000_000;

pub(crate) const CONCAT_FILE_NAME: &str = "frames.ffconcat";
pub(crate) const OUTPUT_FILE_NAME: &str = "output.partial.mp4";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum H264Encoder {
    Libx264,
}

impl H264Encoder {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Libx264 => "libx264",
        }
    }

    pub(crate) fn append_arguments(
        self,
        command: &mut Vec<OsString>,
        geometry: VideoOutputGeometry,
        presentation_starts_micros: &[u64],
        duration_micros: u64,
        output_limit: u64,
    ) {
        let scaled = geometry.scaled();
        let canvas = geometry.canvas();
        let presentation_timestamps = presentation_timestamps_filter(
            presentation_starts_micros,
            duration_micros.saturating_sub(1),
        );
        let filter = format!(
            "settb=expr=1/{FFMPEG_TIMEBASE_HZ},setpts={presentation_timestamps}/{FFMPEG_TIMEBASE_HZ}/TB,scale=w={}:h={}:flags=lanczos,pad=w={}:h={}:x=0:y=0:color=black",
            scaled.width(),
            scaled.height(),
            canvas.width(),
            canvas.height()
        );
        command.extend([
            "-nostdin".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-filter_threads".into(),
            "1".into(),
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "1".into(),
            "-r".into(),
            FFMPEG_TIMEBASE_HZ.to_string().into(),
            "-i".into(),
            CONCAT_FILE_NAME.into(),
            "-map".into(),
            "0:v:0".into(),
            "-an".into(),
            "-sn".into(),
            "-dn".into(),
            "-fps_mode".into(),
            "vfr".into(),
            "-vf".into(),
            filter.into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:v".into(),
            self.name().into(),
            "-threads".into(),
            "1".into(),
            "-g".into(),
            "1".into(),
            "-keyint_min".into(),
            "1".into(),
            "-sc_threshold".into(),
            "0".into(),
            "-enc_time_base".into(),
            "1:1000000".into(),
            "-video_track_timescale".into(),
            FFMPEG_TIMEBASE_HZ.to_string().into(),
            "-map_metadata".into(),
            "-1".into(),
            "-map_chapters".into(),
            "-1".into(),
            "-metadata".into(),
            "encoder=".into(),
            "-metadata".into(),
            "creation_time=".into(),
            "-t".into(),
            format!(
                "{}.{:06}",
                duration_micros / 1_000_000,
                duration_micros % 1_000_000
            )
            .into(),
            "-movflags".into(),
            "+faststart".into(),
            "-fs".into(),
            output_limit.to_string().into(),
            "-y".into(),
            OUTPUT_FILE_NAME.into(),
        ]);
    }
}

pub(crate) fn encode_arguments(
    geometry: VideoOutputGeometry,
    presentation_starts_micros: &[u64],
    duration_micros: u64,
    output_limit: u64,
) -> Vec<OsString> {
    let mut arguments = Vec::with_capacity(60);
    H264Encoder::Libx264.append_arguments(
        &mut arguments,
        geometry,
        presentation_starts_micros,
        duration_micros,
        output_limit,
    );
    arguments
}

fn presentation_timestamps_filter(starts_micros: &[u64], terminal_micros: u64) -> String {
    starts_micros
        .iter()
        .enumerate()
        .rev()
        .fold(terminal_micros.to_string(), |otherwise, (index, start)| {
            format!("if(eq(N\\,{index})\\,{start}\\,{otherwise})")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::PixelDimensions;

    #[test]
    fn v1_arguments_are_one_exact_allowlisted_sequence() {
        let source = PixelDimensions::new(5, 3).unwrap();
        let scaled = PixelDimensions::new(5, 3).unwrap();
        let canvas = PixelDimensions::new(6, 4).unwrap();
        let geometry = VideoOutputGeometry::new(source, scaled, canvas).unwrap();
        let arguments = encode_arguments(geometry, &[0, 100_000], 350_001, 8_192);
        let actual: Vec<_> = arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(actual.first().map(String::as_str), Some("-nostdin"));
        assert_eq!(actual.last().map(String::as_str), Some(OUTPUT_FILE_NAME));
        assert_eq!(actual.iter().filter(|value| *value == "libx264").count(), 1);
        assert_eq!(
            actual.iter().filter(|value| value.as_str() == "-i").count(),
            1
        );
        assert!(actual.windows(2).any(|pair| pair == ["-safe", "1"]));
        assert!(actual.windows(2).any(|pair| pair == ["-r", "1000000"]));
        assert!(actual.windows(2).any(|pair| pair == ["-fs", "8192"]));
        assert!(actual.windows(2).any(|pair| pair == ["-t", "0.350001"]));
        assert!(
            actual.contains(&"settb=expr=1/1000000,setpts=if(eq(N\\,0)\\,0\\,if(eq(N\\,1)\\,100000\\,350000))/1000000/TB,scale=w=5:h=3:flags=lanczos,pad=w=6:h=4:x=0:y=0:color=black".to_owned())
        );
        assert!(!actual.iter().any(|value| {
            value.starts_with('/')
                || value.starts_with('\\')
                || (value
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
                    && value.as_bytes().get(1) == Some(&b':'))
        }));
    }

    #[test]
    fn presentation_filter_assigns_each_source_and_the_terminal_sentinel() {
        assert_eq!(
            presentation_timestamps_filter(&[0, 100_000, 225_000], 349_999),
            "if(eq(N\\,0)\\,0\\,if(eq(N\\,1)\\,100000\\,if(eq(N\\,2)\\,225000\\,349999)))"
        );
    }
}
