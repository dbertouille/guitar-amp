use anyhow::Result;
use cpal::Device;
use cpal::traits::{DeviceTrait, HostTrait};
use std::io::{self, Write};
use tracing::{info, warn};

/// Runtime configuration for the amp
pub(crate) struct Config {
    /// The input (capture) device chosen by the user.
    pub(crate) input_device: Device,
    /// The output (playback) device chosen by the user.
    pub(crate) output_device: Device,

    /// Capacity of the ring buffer between the input and output threads, in
    /// samples. Must stay comfortably larger than the prefill
    /// (`frames_per_buffer * 2`): the prefill is fixed latency we want small,
    /// while the extra headroom absorbs jitter between the two device clocks.
    /// 1024 leaves plenty of room above the 256-sample prefill for the default
    /// `frames_per_buffer`. If you raise `frames_per_buffer` substantially,
    /// raise this too so the prefill doesn't crowd the capacity.
    pub(crate) ring_buffer_size: usize,

    /// Amp gain/distortion amount.
    pub(crate) gain: f32,

    /// Requested device buffer size (frames per callback). Smaller = lower
    /// latency but more CPU/risk of underruns. 128 frames @ 48kHz ≈ 2.7ms.
    pub(crate) frames_per_buffer: u32,
}

impl Config {
    /// Build a config from interactive device selection, using default values
    /// for the tuning knobs.
    pub(crate) fn from_prompt(host: &cpal::Host) -> Result<Self> {
        // Let the user pick the input device.
        let input_devices: Vec<Device> = host.input_devices()?.collect();
        let input_device = select_device(&input_devices, "input")?;

        // Let the user pick the output device.
        let output_devices: Vec<Device> = host.output_devices()?.collect();
        let output_device = select_device(&output_devices, "output")?;

        Ok(Self {
            input_device,
            output_device,
            ring_buffer_size: 1024,
            gain: 5.0,
            frames_per_buffer: 128,
        })
    }
}

/// Prompt the user to choose a device from the given list by index.
/// Pressing Enter with no input selects the default (first) device.
fn select_device(devices: &[Device], kind: &str) -> Result<Device> {
    if devices.is_empty() {
        anyhow::bail!("No {} device available", kind);
    }

    info!("available {kind} devices:");
    for (idx, device) in devices.iter().enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let default_marker = if idx == 0 { " (default)" } else { "" };
        info!("  [{idx}] {name}{default_marker}");
    }

    loop {
        // This is the interactive input prompt, not a log line: it deliberately
        // omits a trailing newline so the user's typed answer sits on the same
        // line. A tracing event always emits a full line, so it must stay a
        // direct stdout write.
        print!(
            "\nSelect {} device [0-{}] (Enter for default): ",
            kind,
            devices.len() - 1
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Ok(devices[0].clone());
        }

        match trimmed.parse::<usize>() {
            Ok(idx) if idx < devices.len() => return Ok(devices[idx].clone()),
            _ => warn!(
                "invalid selection — enter a number between 0 and {}",
                devices.len() - 1
            ),
        }
    }
}
