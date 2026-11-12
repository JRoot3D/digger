//! `cargo xtask render-sound <script> <out.wav>`: renders a sound command
//! script through the sound engine to a WAV file (design D10 and D12 of
//! `add-sound-engine`).
//!
//! The script format is described in `digger_sound::script`: one
//! `<sample offset> <command>` entry per line, ending with `<offset> end`.
//! The render uses `digger_sound::ManualDriver`, so it is the same render
//! the golden tests hash when run with their settings (44100 Hz, filter
//! off, volume 100, which are this command's defaults).

use std::fs;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use digger_sound::{SampleRate, Script, SoundConfig, TuneTables, Volume};

/// Arguments of `cargo xtask render-sound`.
#[derive(Debug, Args)]
pub(crate) struct RenderSoundArgs {
    /// The command script to render.
    pub(crate) script: PathBuf,
    /// Where to write the 16-bit mono WAV file (parent directories are
    /// created).
    pub(crate) out: PathBuf,
    /// The sample rate in Hz (8000 to 192000). Script offsets are samples
    /// at this rate.
    #[arg(long, default_value_t = 44100)]
    pub(crate) rate: u32,
    /// Whether the output filter (high-pass 1 kHz, low-pass 4 kHz) is on.
    #[arg(long, value_enum, default_value_t = Switch::Off)]
    pub(crate) filter: Switch,
    /// The master volume in percent; values above 100 clamp to 100.
    #[arg(long, default_value_t = 100)]
    pub(crate) volume: u8,
}

/// An on/off command-line value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Switch {
    /// On.
    On,
    /// Off.
    Off,
}

/// Runs `cargo xtask render-sound`.
pub(crate) fn run(args: &RenderSoundArgs) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(&args.script)
        .map_err(|e| format!("cannot read {}: {e}", args.script.display()))?;
    let script: Script = text
        .parse()
        .map_err(|e| format!("{}: {e}", args.script.display()))?;
    let rate = SampleRate::new(args.rate)?;
    let config = SoundConfig {
        sample_rate: rate,
        filter: args.filter == Switch::On,
        volume: Volume::new(args.volume.min(100)),
        ..SoundConfig::default()
    };
    let samples = script.render(config, TuneTables::embedded()?);
    let bytes = wav_bytes(&samples, rate.hz())?;
    if let Some(dir) = args.out.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::write(&args.out, bytes).map_err(|e| format!("cannot write {}: {e}", args.out.display()))?;
    eprintln!(
        "xtask render-sound: wrote {} ({} samples, {:.2} s at {} Hz)",
        args.out.display(),
        samples.len(),
        f64::from(u32::try_from(samples.len()).unwrap_or(u32::MAX)) / f64::from(rate.hz()),
        rate.hz()
    );
    Ok(())
}

/// The length of the canonical WAV header written by [`wav_bytes`].
pub(crate) const WAV_HEADER_LEN: usize = 44;

/// A canonical 44-byte-header PCM WAV file: one channel, 16-bit
/// little-endian samples at `rate` Hz.
///
/// Fails if the data does not fit the format's 32-bit sizes.
pub(crate) fn wav_bytes(samples: &[i16], rate: u32) -> Result<Vec<u8>, String> {
    const CHANNELS: u16 = 1;
    const BITS: u16 = 16;
    let block_align = CHANNELS * BITS / 8;
    let data_len = samples
        .len()
        .checked_mul(usize::from(block_align))
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n <= u32::MAX - 36)
        .ok_or("too many samples for a WAV file")?;
    let byte_rate = rate
        .checked_mul(u32::from(block_align))
        .ok_or("sample rate too high for a WAV file")?;
    let mut out = Vec::with_capacity(WAV_HEADER_LEN + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn wav_header_is_valid() {
        let samples = [0_i16, 16383, -16383, i16::MIN, i16::MAX];
        let bytes = wav_bytes(&samples, 44100).expect("small file");
        assert_eq!(bytes.len(), WAV_HEADER_LEN + 10);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32_at(&bytes, 4), 36 + 10);
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32_at(&bytes, 16), 16);
        assert_eq!(u16_at(&bytes, 20), 1, "PCM");
        assert_eq!(u16_at(&bytes, 22), 1, "mono");
        assert_eq!(u32_at(&bytes, 24), 44100);
        assert_eq!(u32_at(&bytes, 28), 88200, "byte rate");
        assert_eq!(u16_at(&bytes, 32), 2, "block align");
        assert_eq!(u16_at(&bytes, 34), 16, "bits per sample");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32_at(&bytes, 40), 10);
        let data: Vec<i16> = bytes[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&b| i16::from_le_bytes(b))
            .collect();
        assert_eq!(data, samples);
    }

    #[test]
    fn empty_wav_is_header_only() {
        let bytes = wav_bytes(&[], 8000).expect("empty file");
        assert_eq!(bytes.len(), WAV_HEADER_LEN);
        assert_eq!(u32_at(&bytes, 4), 36);
        assert_eq!(u32_at(&bytes, 40), 0);
    }

    #[test]
    fn renders_the_gold_script() {
        let dir = std::env::temp_dir().join(format!("xtask-render-sound-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let script = dir.join("gold.txt");
        fs::write(&script, "0 gold\n21_210 end\n").expect("script");
        let out = dir.join("out/gold.wav");
        let args = RenderSoundArgs {
            script,
            out: out.clone(),
            rate: 44100,
            filter: Switch::Off,
            volume: 100,
        };
        run(&args).expect("render");
        let bytes = fs::read(&out).expect("wav");
        assert_eq!(bytes.len(), WAV_HEADER_LEN + 21_210 * 2);
        assert!(bytes[WAV_HEADER_LEN..].iter().any(|&b| b != 0));
        fs::remove_dir_all(&dir).expect("cleanup");
    }
}
