use ffmpeg_next as ffmpeg;
use ffmpeg::format::sample::{Sample, Type};
use ffmpeg::util::channel_layout::ChannelLayout;

#[derive(Default, Clone)]
pub struct AudioInfo {
    pub path: String,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: f64,
    pub bit_rate: i64,
}

impl AudioInfo {
    pub fn desc(&self) -> String {
        let mut parts = vec![self.codec.clone()];
        if self.bit_rate > 0 {
            parts.push(format!("{} kbps", (self.bit_rate + 500) / 1000));
        }
        if self.sample_rate > 0 {
            parts.push(format!("{} Hz", self.sample_rate));
        }
        if self.channels > 0 {
            parts.push(format!("{} ch", self.channels));
        }
        parts.join(", ")
    }
}

/// Decode to stereo planar f32 at original sample rate.
/// Returns (info, left_samples, right_samples).
/// Mono files are upmixed to stereo (L == R).
pub fn decode(path: &str) -> Result<(AudioInfo, Vec<f32>, Vec<f32>), String> {
    ffmpeg::init().map_err(|e| e.to_string())?;

    let mut ictx = ffmpeg::format::input(&path).map_err(|e| e.to_string())?;

    let stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or_else(|| "No audio stream found".to_string())?;

    let stream_idx = stream.index();
    let time_base = stream.time_base();
    let raw_duration = stream.duration();
    let duration = if raw_duration > 0 {
        raw_duration as f64 * f64::from(time_base)
    } else {
        ictx.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64
    };

    let ctx = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?;
    let mut decoder = ctx.decoder().audio().map_err(|e| e.to_string())?;

    let codec_name = decoder
        .codec()
        .map(|c| c.name().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let info = AudioInfo {
        path: path.to_string(),
        codec: codec_name,
        sample_rate: decoder.rate(),
        channels: decoder.channels(),
        duration,
        bit_rate: decoder.bit_rate() as i64,
    };

    // Resample to stereo planar f32; mono sources are upmixed by ffmpeg
    let mut resampler = decoder
        .resampler(Sample::F32(Type::Planar), ChannelLayout::STEREO, decoder.rate())
        .map_err(|e| e.to_string())?;

    let mut left: Vec<f32> = Vec::new();
    let mut right: Vec<f32> = Vec::new();
    let mut frame = ffmpeg::util::frame::audio::Audio::empty();
    let mut resampled = ffmpeg::util::frame::audio::Audio::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != stream_idx {
            continue;
        }
        decoder.send_packet(&packet).ok();
        while decoder.receive_frame(&mut frame).is_ok() {
            resampler.run(&frame, &mut resampled).ok();
            if resampled.samples() > 0 {
                left.extend_from_slice(resampled.plane::<f32>(0));
                right.extend_from_slice(resampled.plane::<f32>(1));
            }
        }
    }

    decoder.send_eof().ok();
    while decoder.receive_frame(&mut frame).is_ok() {
        resampler.run(&frame, &mut resampled).ok();
        if resampled.samples() > 0 {
            left.extend_from_slice(resampled.plane::<f32>(0));
            right.extend_from_slice(resampled.plane::<f32>(1));
        }
    }

    Ok((info, left, right))
}
