//! The desktop's audio device: cpal's default host and output behind the
//! shared [`Backend`] trait (design D6 of `add-desktop-app`).

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use digger_sound::{AudioSource, SoundHandle};

use crate::platform::audio::{
    AudioRequest, Backend, Capability, DefaultConfig, Feeder, Link, Opened, OutputSample,
    SampleKind, choose,
};

/// The real device: cpal's default host and output.
#[derive(Debug, Default)]
pub(super) struct CpalBackend;

/// A playing cpal stream; dropping it stops the device.
pub(super) struct CpalStream(cpal::Stream);

impl fmt::Debug for CpalStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CpalStream")
    }
}

/// The writable format of a cpal sample format.
fn sample_kind(format: cpal::SampleFormat) -> Option<SampleKind> {
    match format {
        cpal::SampleFormat::I16 => Some(SampleKind::I16),
        cpal::SampleFormat::F32 => Some(SampleKind::F32),
        cpal::SampleFormat::U16 => Some(SampleKind::U16),
        _ => None,
    }
}

/// A cpal buffer-size range as a pair.
const fn buffer_range(size: &cpal::SupportedBufferSize) -> Option<(u32, u32)> {
    match *size {
        cpal::SupportedBufferSize::Range { min, max } => Some((min, max)),
        cpal::SupportedBufferSize::Unknown => None,
    }
}

/// Whether a stream error ends the stream. Under- and overruns, a route
/// change the stream followed and a refused real-time priority leave it
/// playing.
const fn is_fatal(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

/// Builds and starts an output stream writing `T` samples.
fn build<T: cpal::SizedSample + OutputSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut feeder: Feeder<AudioSource>,
    failed: &Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error> {
    let failed = Arc::clone(failed);
    let stream = device.build_output_stream::<T, _, _>(
        config,
        move |out: &mut [T], _| feeder.fill(out),
        move |error: cpal::Error| {
            if is_fatal(error.kind()) {
                failed.store(true, Ordering::Release);
            }
        },
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

impl Backend for CpalBackend {
    type Stream = CpalStream;

    fn pause(stream: &CpalStream) -> Result<(), String> {
        stream.0.pause().map_err(|error| error.to_string())
    }

    fn play(stream: &CpalStream) -> Result<(), String> {
        stream.0.play().map_err(|error| error.to_string())
    }

    fn open(
        &mut self,
        request: AudioRequest,
        link: &mut Link<'_>,
    ) -> Result<Opened<CpalStream>, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| String::from("no default output"))?;
        let configs = device.supported_output_configs();
        let default = device.default_output_config();
        // A device that lists nothing and has no default cannot be used;
        // its own error is the better reason.
        if let (Err(error), Err(_)) = (&configs, &default) {
            return Err(error.to_string());
        }
        let capabilities: Vec<Capability> = configs
            .map(|configs| {
                configs
                    .map(|range| Capability {
                        channels: range.channels(),
                        min_rate: range.min_sample_rate(),
                        max_rate: range.max_sample_rate(),
                        format: sample_kind(range.sample_format()),
                        buffer: buffer_range(range.buffer_size()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let default = default.ok().map(|config| DefaultConfig {
            channels: config.channels(),
            rate: config.sample_rate(),
            format: sample_kind(config.sample_format()),
            buffer: buffer_range(config.buffer_size()),
        });
        let choice = choose(&capabilities, default, request)
            .ok_or_else(|| String::from("the output has no usable sample format"))?;
        let mut notices = choice.notices.clone();
        let failed = Arc::new(AtomicBool::new(false));
        let attempt = |buffer: cpal::BufferSize,
                       link: &mut Link<'_>|
         -> Result<(cpal::Stream, SoundHandle), String> {
            let (handle, source) = link(choice.rate)?;
            let feeder = Feeder::new(source, choice.channels);
            let config = cpal::StreamConfig {
                channels: choice.channels,
                sample_rate: choice.rate,
                buffer_size: buffer,
            };
            let stream = match choice.format {
                SampleKind::I16 => build::<i16>(&device, config, feeder, &failed),
                SampleKind::F32 => build::<f32>(&device, config, feeder, &failed),
                SampleKind::U16 => build::<u16>(&device, config, feeder, &failed),
            }
            .map_err(|error| error.to_string())?;
            Ok((stream, handle))
        };
        let (stream, handle) = match choice.buffer {
            Some(frames) => match attempt(cpal::BufferSize::Fixed(frames), link) {
                Ok(opened) => opened,
                Err(reason) => {
                    notices.push(format!(
                        "the audio device refused a buffer of {frames} frames ({reason}); \
                         using its default"
                    ));
                    attempt(cpal::BufferSize::Default, link)?
                }
            },
            None => attempt(cpal::BufferSize::Default, link)?,
        };
        Ok(Opened {
            stream: CpalStream(stream),
            handle,
            failed,
            notices,
        })
    }
}
