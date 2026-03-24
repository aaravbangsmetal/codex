use image::Delay;
use image::Frame;
use image::RgbaImage;
use image::codecs::gif::GifEncoder;
use image::codecs::gif::Repeat;
use std::fs::File;
use std::path::Path;
pub(crate) struct GifSegmentEncoder {
    encoder: GifEncoder<File>,
    width: u32,
    height: u32,
    frame_delay: Delay,
}

impl GifSegmentEncoder {
    pub(crate) fn open(path: &Path, width: u32, height: u32, fps: u32) -> std::io::Result<Self> {
        let file = File::create(path)?;
        let mut encoder = GifEncoder::new(file);
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(std::io::Error::other)?;
        Ok(Self {
            encoder,
            width,
            height,
            frame_delay: Delay::from_numer_denom_ms(1000, fps.max(1)),
        })
    }

    pub(crate) fn write_rgba_frame(&mut self, rgba: &[u8]) -> std::io::Result<()> {
        let expected_len = self.width as usize * self.height as usize * 4;
        if rgba.len() != expected_len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "rgba frame length {} did not match expected {expected_len}",
                    rgba.len()
                ),
            ));
        }
        let frame =
            RgbaImage::from_raw(self.width, self.height, rgba.to_vec()).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "failed to construct rgba frame from raw bytes",
                )
            })?;
        self.encoder
            .encode_frame(Frame::from_parts(frame, 0, 0, self.frame_delay))
            .map_err(std::io::Error::other)
    }

    pub(crate) fn finish(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for GifSegmentEncoder {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            tracing::debug!("failed to finish gif segment: {error}");
        }
    }
}
