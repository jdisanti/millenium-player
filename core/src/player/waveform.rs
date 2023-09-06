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
use spectrum_analyzer::{samples_fft_to_spectrum, FrequencyLimit};
use std::{
    f32::consts::PI,
    time::{Duration, Instant},
};

const DEFAULT_BINS: usize = 31;

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

impl<const BIN_COUNT: usize> Drop for WaveformCalculator<BIN_COUNT> {
    fn drop(&mut self) {
        log::info!("dropping waveform calculator");
    }
}

impl<const BIN_COUNT: usize> WaveformCalculator<BIN_COUNT> {
    pub fn new(sample_rate: SampleRate) -> Self {
        log::info!(
            "creating waveform calculator with {BIN_COUNT} bins and a sample rate of {sample_rate}"
        );
        Self {
            spectrum: SpectrumCalculator::new(sample_rate),
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
    sample_rate: SampleRate,
    required_samples: usize,
    sample_buffer: Vec<f32>,
    calc_buffer: Vec<f32>,
    output_buffer: [f32; BIN_COUNT],
    last_calculate: Instant,
}

impl<const BIN_COUNT: usize> SpectrumCalculator<BIN_COUNT> {
    fn new(sample_rate: SampleRate) -> Self {
        let required_samples = 8192;
        Self {
            sample_rate,
            required_samples,
            // Allocate a little more than needed since we're getting an entire source
            // buffer at a time, and thus, could exceed the required number of samples.
            sample_buffer: Vec::with_capacity(required_samples + required_samples / 2),
            output_buffer: [0f32; BIN_COUNT],
            last_calculate: Instant::now() - Duration::from_secs(1),
            calc_buffer: vec![0f32; required_samples],
        }
    }

    fn apply_hamming_window(data: &mut [f32]) {
        let len = data.len() as f32;
        for (i, s) in data.iter_mut().enumerate() {
            let w = 0.54 * (2.0 * PI * i as f32 / len).cos();
            *s *= w;
        }
    }

    fn log_max(actual_max_range_hz: f32) -> f32 {
        (actual_max_range_hz - 100.0).log10() - 2.0
    }

    #[inline]
    fn bin(frequency: f32, actual_min_range_hz: f32, log_max: f32) -> usize {
        let log = (frequency - actual_min_range_hz + 100.0).log10() - 2.0;
        let bin = log / log_max * (BIN_COUNT - 1) as f32;
        bin.round() as usize
    }

    pub fn calculate(&mut self) -> bool {
        if self.sample_buffer.len() < self.required_samples {
            return false;
        }
        if Instant::now() - self.last_calculate < Duration::from_millis(1000 / 30) {
            return false;
        }

        // Copy samples into calc buffer
        self.calc_buffer.clear();
        self.calc_buffer.extend(
            self.sample_buffer
                .iter()
                .take(self.required_samples)
                .copied(),
        );

        const MIN_RANGE_HZ: f32 = 20.0;
        let max_range_hz: f32 = f32::min(self.sample_rate as f32 / 2.0, 20_000.0);

        Self::apply_hamming_window(&mut self.calc_buffer);
        let spectrum = samples_fft_to_spectrum(
            &self.calc_buffer,
            self.sample_rate,
            FrequencyLimit::Range(MIN_RANGE_HZ, max_range_hz),
            None,
        )
        .expect("failed to calculate spectrum");
        debug_assert!(spectrum.data().len() > BIN_COUNT);

        let actual_min_range_hz = spectrum.min_fr().val();
        let actual_max_range_hz = spectrum.max_fr().val();
        let log_max = Self::log_max(actual_max_range_hz);

        self.output_buffer.iter_mut().for_each(|v| *v *= 0.3);
        for (freq, value) in spectrum.data().iter() {
            let bin = Self::bin(freq.val(), actual_min_range_hz, log_max);
            let value = (value.val() + 1.0).log10() * 0.3;
            self.output_buffer[bin] = f32::max(self.output_buffer[bin], value);
        }
        self.last_calculate = Instant::now();
        true
    }

    fn push_source(&mut self, source: &SourceBuffer) {
        debug_assert!(source.channel_count() > 0);
        debug_assert_eq!(self.sample_rate, source.sample_rate());

        let source_mono = source.clone().remix(1);
        self.sample_buffer
            .extend(source_mono.channel(0).iter().copied());
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
        // We want the full range of bins to represent one second of audio
        let required_samples = sample_rate as usize / BIN_COUNT;
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
