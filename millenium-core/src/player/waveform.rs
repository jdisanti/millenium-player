// This file is part of Millenium Player.
// Copyright (C) 2023 John DiSanti.
//
// Millenium Player is free software: you can redistribute it and/or modify it under the terms of
// the GNU General Public License as published by the Free Software Foundation, either version 3 of
// the License, or (at your option) any later version.
//
// Millenium Player is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
// the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with Millenium Player.
// If not, see <https://www.gnu.org/licenses/>.

use crate::audio::{source::SourceBuffer, SampleRate};
use std::{
    f32::consts::PI,
    sync::Arc,
    time::{Duration, Instant},
};

const DEFAULT_BINS: usize = 24;

#[derive(Debug)]
pub struct Waveform<const BIN_COUNT: usize = DEFAULT_BINS> {
    last_spectrum_update: Instant,
    last_amplitude_update: Instant,
    pub spectrum: [f32; BIN_COUNT],
    pub amplitude: [f32; BIN_COUNT],
}

impl<const BIN_COUNT: usize> Waveform<BIN_COUNT> {
    pub fn empty() -> Self {
        Self {
            last_spectrum_update: Instant::now() - Duration::from_secs(1),
            last_amplitude_update: Instant::now() - Duration::from_secs(1),
            spectrum: [0f32; BIN_COUNT],
            amplitude: [0f32; BIN_COUNT],
        }
    }

    pub fn copy_from(&mut self, other: &Waveform) {
        self.last_spectrum_update = other.last_spectrum_update;
        self.last_amplitude_update = other.last_amplitude_update;
        self.spectrum.copy_from_slice(&other.spectrum);
        self.amplitude.copy_from_slice(&other.amplitude);
    }
}

// Need a custom serialize because serde derive can't handle `[f32; BIN_COUNT]`
impl<const BIN_COUNT: usize> serde::Serialize for Waveform<BIN_COUNT> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("spectrum", &self.spectrum[..])?;
        map.serialize_entry("amplitude", &self.amplitude[..])?;
        map.end()
    }
}

pub struct WaveformCalculator<const BIN_COUNT: usize = DEFAULT_BINS> {
    spectrum: SpectrumCalculator<BIN_COUNT>,
    amplitude: AmplitudeCalculator<BIN_COUNT>,
}

impl<const BIN_COUNT: usize> WaveformCalculator<BIN_COUNT> {
    pub fn new(sample_rate: SampleRate) -> Self {
        Self {
            spectrum: SpectrumCalculator::new(),
            amplitude: AmplitudeCalculator::new(sample_rate),
        }
    }

    pub fn waveform_needs_update(&self, waveform: &Waveform) -> bool {
        waveform.last_spectrum_update < self.spectrum.last_calculate
            || waveform.last_amplitude_update < self.amplitude.last_calculate
    }

    /// Returns true if the waveform was updated.
    pub fn calculate(&mut self) {
        self.spectrum.calculate();
        self.amplitude.calculate();
    }

    pub fn push_source(&mut self, source: &SourceBuffer) {
        self.spectrum.push_source(source);
        self.amplitude.push_source(source);
    }

    pub fn copy_latest_waveform_into(&self, waveform: &mut Waveform<BIN_COUNT>) {
        self.spectrum.copy_latest_waveform_into(waveform);
        self.amplitude.copy_latest_waveform_into(waveform);
    }
}

struct SpectrumCalculator<const BIN_COUNT: usize> {
    required_samples: usize,
    sample_buffer: Vec<f32>,
    output_buffer: [f32; BIN_COUNT],
    last_calculate: Instant,

    fft_plan: Arc<dyn rustfft::Fft<f32>>,
    fft_buffer: Vec<rustfft::num_complex::Complex<f32>>,
    fft_scratch: Vec<rustfft::num_complex::Complex<f32>>,
    fft_window: Box<[f32]>,
}

impl<const BIN_COUNT: usize> SpectrumCalculator<BIN_COUNT> {
    fn new() -> Self {
        // Number of frequency bins = required samples / 2 + 1
        //
        // The output of the Fast Fourier Transform is (mostly) symmetric (or a mirror image)
        // around the center point, and only the second half of the output will be used.
        //
        // Thus, to get the output to match our bin count, the required samples must be
        // 2 * waveform size - 1.
        let required_samples = BIN_COUNT * 2 - 1;

        let mut planner = rustfft::FftPlanner::<f32>::new();
        let fft_plan = planner.plan_fft_forward(required_samples);
        let fft_buffer = vec![rustfft::num_complex::Complex::default(); required_samples];
        let fft_scratch =
            vec![rustfft::num_complex::Complex::default(); fft_plan.get_inplace_scratch_len()];
        let fft_window = (0..required_samples)
            .map(|i| {
                // Hamming window function
                0.54 * (1.0 - (2.0 * PI * i as f32 / required_samples as f32).cos())
            })
            .collect();
        Self {
            required_samples,
            // Allocate a little more than needed since we're getting an entire source
            // buffer at a time, and thus, could exceed the required number of samples.
            sample_buffer: Vec::with_capacity(required_samples + required_samples / 2),
            output_buffer: [0f32; BIN_COUNT],
            last_calculate: Instant::now() - Duration::from_secs(1),

            fft_plan,
            fft_buffer,
            fft_scratch,
            fft_window,
        }
    }

    pub fn calculate(&mut self) -> bool {
        if self.sample_buffer.len() < self.required_samples {
            return false;
        }

        let sample_to_complex = |(s, w)| rustfft::num_complex::Complex::new(s * w, 0.0);

        // Fill the FFT buffer with the samples, with the window applied.
        self.fft_buffer.clear();
        // Drain the first half of the required samples into the FFT buffer
        self.fft_buffer.extend(
            self.sample_buffer
                .drain(..(self.required_samples / 2))
                .zip(self.fft_window.iter().copied())
                .map(sample_to_complex),
        );
        // Copy the second half into the FFT buffer.
        self.fft_buffer.extend(
            self.sample_buffer
                .iter()
                .take(self.required_samples / 2 + 1)
                .copied()
                .zip(self.fft_window.iter().copied().skip(self.fft_buffer.len()))
                .map(sample_to_complex),
        );

        self.fft_plan
            .process_with_scratch(&mut self.fft_buffer, &mut self.fft_scratch);
        let scale = 1.0 / (self.fft_buffer.len() as f32).sqrt();
        let mut calc_iter = self.output_buffer.iter_mut().zip(
            self.fft_buffer
                .iter()
                .rev()
                .skip(2)
                .take(BIN_COUNT)
                // Convert complex to magnitude
                .map(|c| c.norm() * scale)
                // Clamp
                .map(|s| f32::min(1.0, f32::max(0f32, s))),
        );
        for (out, calc) in &mut calc_iter {
            *out = calc;
        }
        self.last_calculate = Instant::now();
        true
    }

    fn push_source(&mut self, source: &SourceBuffer) {
        self.sample_buffer.extend(source.channel(0).iter().copied());
        if self.sample_buffer.len() > self.required_samples {
            self.sample_buffer
                .drain(..(self.sample_buffer.len() - self.required_samples));
        }
    }

    pub fn copy_latest_waveform_into(&self, waveform: &mut Waveform<BIN_COUNT>) {
        if waveform.last_spectrum_update < self.last_calculate {
            waveform.last_spectrum_update = self.last_calculate;
            waveform.spectrum.copy_from_slice(&self.output_buffer);
        }
    }
}

struct AmplitudeCalculator<const BIN_COUNT: usize> {
    #[cfg(debug_assertions)]
    sample_rate: SampleRate,

    required_samples: usize,
    sample_buffer: Vec<f32>,
    output_buffer: [f32; BIN_COUNT],
    last_calculate: Instant,
}

impl<const BIN_COUNT: usize> AmplitudeCalculator<BIN_COUNT> {
    fn new(sample_rate: SampleRate) -> Self {
        // We want the bins to represent a quarter second of audio
        let required_samples = (sample_rate as usize / 4) / BIN_COUNT;
        Self {
            #[cfg(debug_assertions)]
            sample_rate,

            required_samples,
            // Allocate a little more than needed since we're getting an entire source
            // buffer at a time, and thus, could exceed the required number of samples.
            sample_buffer: Vec::with_capacity(required_samples + required_samples / 2),
            output_buffer: [0f32; BIN_COUNT],
            last_calculate: Instant::now() - Duration::from_secs(1),
        }
    }

    pub fn calculate(&mut self) -> bool {
        if self.sample_buffer.len() < self.required_samples {
            return false;
        }
        let to_process = self.sample_buffer.drain(..self.required_samples);
        let sum: f32 = to_process.sum();
        let amplitude = f32::min(1.0, 2.0 * sum / self.required_samples as f32);
        self.push_calculation(amplitude);
        self.last_calculate = Instant::now();
        self.sample_buffer.clear();
        true
    }

    fn push_calculation(&mut self, value: f32) {
        self.output_buffer.rotate_left(1);
        *self.output_buffer.iter_mut().next_back().unwrap() = value;
    }

    fn push_source(&mut self, source: &SourceBuffer) {
        debug_assert!(source.channel_count() > 0);
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.sample_rate, source.sample_rate());

        if source.channel_count() == 1 {
            self.sample_buffer
                .extend(source.channel(0).iter().copied().map(f32::abs));
        } else {
            // In stereo (or higher), take the max amplitudes of the first two channels
            self.sample_buffer.extend(
                source
                    .channel(0)
                    .iter()
                    .copied()
                    .map(f32::abs)
                    .zip(source.channel(1).iter().copied().map(f32::abs))
                    .map(|(l, r)| f32::max(l, r)),
            );
        }
    }

    pub fn copy_latest_waveform_into(&self, waveform: &mut Waveform<BIN_COUNT>) {
        if waveform.last_amplitude_update < self.last_calculate {
            waveform.last_amplitude_update = self.last_calculate;
            waveform.amplitude.copy_from_slice(&self.output_buffer);
        }
    }
}
