mod config;

use anyhow::Result;
use config::Config;
use cpal::BufferSize;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use ringbuf::{HeapCons, HeapProd, HeapRb, traits::*};
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    // Initialize structured logging. Honors RUST_LOG (e.g. RUST_LOG=debug),
    // defaulting to `info` when unset.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("🎸 Guitar Amp starting");

    let host = cpal::default_host();
    let config = Config::from_prompt(&host)?;

    info!(input = %config.input_device.name()?, "using input device");
    info!(output = %config.output_device.name()?, "using output device");

    // Get supported configurations
    let input_config = config.input_device.default_input_config()?;
    let output_config = config.output_device.default_output_config()?;

    info!(?input_config, "resolved input config");
    info!(?output_config, "resolved output config");

    let input_channels = input_config.channels() as usize;
    let output_channels = output_config.channels() as usize;

    // We pass samples straight through the ring buffer without resampling, so the
    // two devices must agree on sample rate. If they differ, every sample is
    // consumed at a different rate than it was produced, which shifts the pitch
    // (and slowly drifts the buffer toward over/underrun). Refuse rather than
    // produce audibly wrong output.
    if input_config.sample_rate() != output_config.sample_rate() {
        anyhow::bail!(
            "input ({} Hz) and output ({} Hz) sample rates differ; \
             resampling is not supported — pick devices with matching sample rates",
            input_config.sample_rate().0,
            output_config.sample_rate().0
        );
    }

    // Create ring buffer for audio transfer. Holds mono frames; we mix down on
    // input and fan out to all output channels on the way out. ringbuf's
    // producer/consumer halves are lock-free SPSC, so no Mutex is needed (and
    // locking on the realtime audio thread would risk glitches anyway).
    let ring_buffer = HeapRb::<f32>::new(config.ring_buffer_size);
    let (mut producer, consumer) = ring_buffer.split();

    // Pre-fill with a small amount of silence to absorb jitter between the two
    // clocks. Keep this as low as possible: every prefilled sample is fixed
    // latency. Two device buffers' worth is a reasonable safety margin.
    for _ in 0..(config.frames_per_buffer as usize * 2) {
        let _ = producer.try_push(0.0);
    }

    // Build streams, requesting a small buffer size for low latency.
    let mut input_stream_config: StreamConfig = input_config.into();
    input_stream_config.buffer_size = BufferSize::Fixed(config.frames_per_buffer);
    let mut output_stream_config: StreamConfig = output_config.into();
    output_stream_config.buffer_size = BufferSize::Fixed(config.frames_per_buffer);

    let input_stream = build_input_stream(
        &config.input_device,
        &input_stream_config,
        producer,
        input_channels,
    )?;
    let output_stream = build_output_stream(
        &config.output_device,
        &output_stream_config,
        consumer,
        output_channels,
        config.gain,
    )?;

    // Start streams
    input_stream.play()?;
    output_stream.play()?;

    info!(
        gain = config.gain,
        "🎵 guitar amp is running — plug in and play; press Ctrl+C to quit"
    );

    // Keep the program running
    std::thread::park();

    Ok(())
}

fn build_input_stream(
    device: &Device,
    config: &StreamConfig,
    mut producer: HeapProd<f32>,
    channels: usize,
) -> Result<Stream> {
    let err_fn = |err| tracing::error!(%err, "input stream error");

    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Push one mono sample per frame, averaging across input channels.
            for frame in data.chunks(channels) {
                let mono = frame.iter().sum::<f32>() / channels as f32;
                // If buffer is full, drop the sample (shouldn't happen often)
                let _ = producer.try_push(mono);
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_output_stream(
    device: &Device,
    config: &StreamConfig,
    mut consumer: HeapCons<f32>,
    channels: usize,
    gain: f32,
) -> Result<Stream> {
    let err_fn = |err| tracing::error!(%err, "output stream error");

    let stream = device.build_output_stream(
        config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // Pop one mono sample per output frame and fan it out to every channel.
            for frame in data.chunks_mut(channels) {
                let input_sample = consumer.try_pop().unwrap_or(0.0);
                let processed = apply_amp_effect(input_sample, gain);
                for sample in frame.iter_mut() {
                    *sample = processed;
                }
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn apply_amp_effect(sample: f32, gain: f32) -> f32 {
    // Apply gain
    let gained = sample * gain;

    // Soft clipping (tanh distortion) - simulates tube amp saturation
    let distorted = gained.tanh();

    // Scale down a bit to prevent clipping
    distorted * 0.7
}
