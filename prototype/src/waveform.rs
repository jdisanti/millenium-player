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
use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, Barrier, Mutex},
    thread,
    time::Duration,
};

struct Waveform {
    waveform_size: usize,
    waveform: Vec<f32>,
    samples: Vec<i16>,
    samples_per_waveform: usize,
}
impl Waveform {
    fn new(waveform_size: usize) -> Self {
        Self {
            waveform_size,
            waveform: vec![0f32; waveform_size],
            samples: Vec::new(),
            samples_per_waveform: 0,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: usize) {
        // Multiply by two so that the waveform shows two seconds worth of audio
        self.samples_per_waveform = sample_rate / self.waveform_size * 2;
    }

    fn update(&mut self, next: i16) {
        debug_assert!(
            self.samples_per_waveform > 0,
            "set_sample_rate must be called before update"
        );
        self.samples.push(next);
        if self.samples.len() >= self.samples_per_waveform {
            self.calculate_next();
            self.samples.clear();
        }
    }

    fn calculate_next(&mut self) {
        let sum: f32 = self.samples.iter().map(|&s| f32::abs(s as f32)).sum();
        self.waveform.remove(0);
        self.waveform.push(f32::min(
            1.0,
            (sum / self.samples_per_waveform as f32) / (i16::MAX as f32 / 2.0),
        ));
    }

    fn heights(&self) -> &[f32] {
        debug_assert!(self.waveform.len() == self.waveform_size);
        &self.waveform
    }

    fn size(&self) -> usize {
        self.waveform_size
    }
}

type SharedWaveform = Arc<Mutex<Waveform>>;

struct WaveformAudioSource {
    decoder: rodio::Decoder<BufReader<File>>,
    waveform: SharedWaveform,
}

impl WaveformAudioSource {
    fn new(path: impl AsRef<Path>, waveform: SharedWaveform) -> Self {
        let path = path.as_ref();
        let decoder = Self::load(path);
        waveform
            .lock()
            .unwrap()
            .set_sample_rate(decoder.sample_rate() as usize);
        Self { decoder, waveform }
    }

    fn load(path: &Path) -> rodio::Decoder<BufReader<File>> {
        rodio::Decoder::new(BufReader::new(File::open(path).unwrap())).unwrap()
    }
}

impl Iterator for WaveformAudioSource {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.decoder.next();
        if let Some(next) = next {
            self.waveform.lock().unwrap().update(next);
        }
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
    waveform_heights: Vec<f32>,
}

impl App {
    fn new(_creation_context: &eframe::CreationContext<'_>, waveform: SharedWaveform) -> Self {
        let waveform_size = waveform.lock().unwrap().size();
        Self {
            waveform,
            waveform_heights: Vec::with_capacity(waveform_size),
        }
    }
}

fn waveform_ui(ui: &mut egui::Ui, waveform_heights: &[f32]) {
    ui.horizontal_centered(|ui| {
        let available_size = ui.available_size();
        let (_, outer_rect) = ui.allocate_space(available_size);
        let width = outer_rect.width() / waveform_heights.len() as f32;
        for (i, height) in waveform_heights.iter().enumerate() {
            let mut rect = outer_rect;
            let start = width * 0.2 + i as f32 * width;
            rect.set_left(start);
            rect.set_right(start + width * 0.8);
            rect.set_top(available_size.y * (1.0 - height));

            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect(rect, 0.1, Color32::BLUE, (0.0, Color32::RED));
            }
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.waveform_heights.clear();
            self.waveform_heights
                .extend(self.waveform.lock().unwrap().heights());
            waveform_ui(ui, &self.waveform_heights);
        });

        ctx.request_repaint();
    }
}

pub fn prototype_waveform() {
    let waveform = Arc::new(Mutex::new(Waveform::new(32)));

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
