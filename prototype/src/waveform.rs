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

use egui::Color32;
use rodio::Source as _;
use rustfft::num_complex::Complex;
use std::{
    f32::consts::PI,
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, Barrier, Mutex},
    thread,
    time::Duration,
};

// Which window function to use to taper the ends of the sample data before FFT
// See: https://en.wikipedia.org/wiki/Window_function
#[derive(Copy, Clone)]
enum WindowFunction {
    None,
    Hamming,
}

impl WindowFunction {
    fn load_values(self, into: &mut [f32]) {
        let len = into.len() as f32;
        match self {
            Self::None => {}
            Self::Hamming => {
                for (i, s) in into.iter_mut().enumerate() {
                    *s = 0.54 * (1.0 - (2.0 * PI * i as f32 / len).cos());
                }
            }
        }
    }
}

struct RealFftCalculator {
    plan: Arc<dyn rustfft::Fft<f32>>,
    samples: Vec<f32>,
    input: Vec<rustfft::num_complex::Complex<f32>>,
    window: Vec<f32>,
    output_magnitude: Vec<f32>,
    scratch: Vec<rustfft::num_complex::Complex<f32>>,
    frame_size: usize,
    hop_size: usize,
    current: usize,
}

impl RealFftCalculator {
    fn new(window_function: WindowFunction, n: usize) -> Self {
        let mut planner = rustfft::FftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(n);
        let mut window = vec![0f32; n];
        window_function.load_values(&mut window);
        Self {
            plan: plan.clone(),
            samples: Vec::new(),
            input: Vec::new(),
            window,
            output_magnitude: Vec::new(),
            scratch: vec![Complex::default(); plan.get_inplace_scratch_len()],
            frame_size: n,
            hop_size: n / 2,
            current: 0,
        }
    }

    fn push_sample(&mut self, sample: f32) -> bool {
        self.samples.push(sample);
        if self.samples.len() > self.frame_size {
            self.samples.remove(0);
        }
        debug_assert!(self.samples.len() <= self.frame_size);
        self.current += 1;
        if self.samples.len() == self.frame_size && self.current >= self.hop_size {
            self.current = 0;
            true
        } else {
            false
        }
    }

    fn calculate(&mut self) -> &[f32] {
        self.input = self
            .samples
            .iter()
            .copied()
            .enumerate()
            .map(|(i, s)| Complex::new(s * self.window[i], 0.0))
            .collect();
        self.plan
            .process_with_scratch(&mut self.input, &mut self.scratch);

        self.output_magnitude = self.input.iter().map(|c| c.norm_sqr()).collect();

        // Convert magnitude to decibels
        for value in &mut self.output_magnitude {
            *value = f32::max(0f32, 10.0 * value.log10());
        }
        &self.output_magnitude
    }
}

struct Waveform {
    waveform_size: usize,
    time_domain_waveform: Vec<f32>,
    time_domain_samples: Vec<f32>,
    freq_domain_waveform: Vec<f32>,
    freq_domain_samples: Vec<f32>,
    samples_per_waveform: usize,
    real_fft_calc: Option<RealFftCalculator>,
}
impl Waveform {
    fn new(waveform_size: usize) -> Self {
        Self {
            waveform_size,
            time_domain_waveform: vec![0f32; waveform_size],
            time_domain_samples: Vec::new(),
            freq_domain_waveform: vec![0f32; waveform_size],
            freq_domain_samples: Vec::new(),
            samples_per_waveform: 0,
            real_fft_calc: None,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: usize) {
        self.samples_per_waveform = sample_rate / self.waveform_size;

        // Number of frequency bins = frame size / 2 + 1
        //
        // The output of the Fast Fourier Transform is (mostly) symmetric (or a mirror image)
        // around the center point, and only the second half of the output will be used.
        //
        // Thus, to get the output to match our waveform size, the frame size must be
        // 2 * waveform size - 1.
        self.real_fft_calc = Some(RealFftCalculator::new(
            WindowFunction::Hamming,
            self.waveform_size * 2 - 1,
        ));
    }

    fn update(&mut self, next: i16) {
        debug_assert!(
            self.samples_per_waveform > 0,
            "set_sample_rate must be called before update"
        );
        let sample = next as f32 / i16::MAX as f32;

        let ready_to_calculate_fft = self
            .real_fft_calc
            .as_mut()
            .expect("set in set_sample_rate")
            .push_sample(next as f32);
        if ready_to_calculate_fft {
            self.calculate_freq_domain();
        }

        self.time_domain_samples.push(sample);
        if self.time_domain_samples.len() >= self.samples_per_waveform {
            self.calculate_time_domain();
            self.time_domain_samples.clear();
        }
    }

    fn calculate_time_domain(&mut self) {
        let sum: f32 = self.time_domain_samples.iter().map(|&s| f32::abs(s)).sum();
        self.time_domain_waveform.remove(0);
        self.time_domain_waveform
            .push(f32::min(1.0, 2.0 * sum / self.samples_per_waveform as f32));
    }

    fn calculate_freq_domain(&mut self) {
        let spectrum = self
            .real_fft_calc
            .as_mut()
            .expect("set in set_sample_rate")
            .calculate();

        self.freq_domain_samples.clear();
        self.freq_domain_samples.extend(spectrum.iter());

        self.freq_domain_waveform.clear();
        self.freq_domain_waveform.extend(
            spectrum
                .iter()
                .rev()
                .take(self.waveform_size)
                .map(|s| s / 110.0),
        );
    }

    fn time_domain_heights(&self) -> &[f32] {
        debug_assert!(self.time_domain_waveform.len() == self.waveform_size);
        &self.time_domain_waveform
    }

    fn freq_domain_heights(&self) -> &[f32] {
        debug_assert!(self.freq_domain_waveform.len() == self.waveform_size);
        &self.freq_domain_waveform
    }

    fn size(&self) -> usize {
        self.waveform_size
    }
}

type SharedWaveform = Arc<Mutex<Waveform>>;

struct WaveformAudioSource {
    decoder: rodio::Decoder<BufReader<File>>,
    waveform: SharedWaveform,
    next_channel: usize,
}

impl WaveformAudioSource {
    fn new(path: impl AsRef<Path>, waveform: SharedWaveform) -> Self {
        let path = path.as_ref();
        let decoder = Self::load(path);
        waveform
            .lock()
            .unwrap()
            .set_sample_rate(decoder.sample_rate() as usize);
        Self {
            decoder,
            waveform,
            next_channel: 0,
        }
    }

    fn load(path: &Path) -> rodio::Decoder<BufReader<File>> {
        rodio::Decoder::new(BufReader::new(File::open(path).unwrap())).unwrap()
    }
}

impl Iterator for WaveformAudioSource {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.decoder.next();
        if self.next_channel == 0 {
            if let Some(next) = next {
                self.waveform.lock().unwrap().update(next);
            }
        }
        self.next_channel = (self.next_channel + 1) % 2;
        next
    }
}

impl rodio::Source for WaveformAudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        self.decoder.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.decoder.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.decoder.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.decoder.total_duration()
    }
}

struct App {
    waveform: SharedWaveform,
    time_domain_heights: Vec<f32>,
    freq_domain_heights: Vec<f32>,
}

impl App {
    fn new(_creation_context: &eframe::CreationContext<'_>, waveform: SharedWaveform) -> Self {
        let waveform_size = waveform.lock().unwrap().size();
        Self {
            waveform,
            time_domain_heights: Vec::with_capacity(waveform_size),
            freq_domain_heights: Vec::with_capacity(waveform_size),
        }
    }
}

fn waveform_ui(ui: &mut egui::Ui, time_domain_heights: &[f32], freq_domain_heights: &[f32]) {
    debug_assert!(time_domain_heights.len() == freq_domain_heights.len());

    ui.horizontal_centered(|ui| {
        let available_size = ui.available_size();
        let (_, outer_rect) = ui.allocate_space(available_size);
        let width = outer_rect.width() / time_domain_heights.len() as f32;

        for (i, (th, fh)) in time_domain_heights
            .iter()
            .zip(freq_domain_heights.iter())
            .enumerate()
        {
            let left = width * 0.2 + i as f32 * width;
            let half_height = available_size.y / 2.0;

            let mut rect = outer_rect;
            rect.set_left(left);
            rect.set_right(left + width * 0.8);

            // Paint frequency domain waveform
            rect.set_top(half_height * (1.0 - fh));
            rect.set_bottom(half_height);
            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect(rect, 0.1, Color32::BLUE, (0.0, Color32::RED));
            }

            // Paint time domain waveform
            rect.set_top(half_height);
            rect.set_bottom(half_height + half_height * th);
            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect(rect, 0.1, Color32::GREEN, (0.0, Color32::GREEN));
            }
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            {
                self.time_domain_heights.clear();
                self.freq_domain_heights.clear();

                let w = self.waveform.lock().unwrap();
                self.time_domain_heights.extend(w.time_domain_heights());
                self.freq_domain_heights.extend(w.freq_domain_heights());
            }
            waveform_ui(ui, &self.time_domain_heights, &self.freq_domain_heights);
        });

        ctx.request_repaint();
    }
}

pub fn prototype_waveform() {
    let waveform = Arc::new(Mutex::new(Waveform::new(24)));

    let native_options = eframe::NativeOptions {
        app_id: Some("millenium-player-prototype".into()),
        drag_and_drop_support: true,
        resizable: true,
        ..Default::default()
    };

    // Use a barrier to make sure the sample rate is set on the waveform before sampling begins
    let barrier = Arc::new(Barrier::new(2));
    thread::spawn({
        let waveform = waveform.clone();
        let barrier = barrier.clone();
        move || {
            let source = WaveformAudioSource::new("../test-data/hydrate/hydrate.mp3", waveform);
            barrier.wait();

            let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
            let sink = rodio::Sink::try_new(&stream_handle).unwrap();
            sink.append(source);
            sink.sleep_until_end();
        }
    });

    barrier.wait();

    eframe::run_native(
        "Millenium Player Prototype",
        native_options,
        Box::new(|cc| Box::new(App::new(cc, waveform))),
    )
    .unwrap();
}
