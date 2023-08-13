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

struct App;
impl App {
    fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        Self
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add(egui::Label::new("Drag and drop a file here"));
            });

            ctx.input(|input| {
                if !input.raw.dropped_files.is_empty() {
                    println!("dropped files: {:?}", input.raw.dropped_files);
                }
            })
        });
    }
}

pub fn prototype_eframe_drag_and_drop() {
    let native_options = eframe::NativeOptions {
        app_id: Some("millenium-player-prototype".into()),
        drag_and_drop_support: true,
        resizable: true,
        ..Default::default()
    };
    eframe::run_native(
        "Millenium Player Prototype",
        native_options,
        Box::new(|cc| Box::new(App::new(cc))),
    )
    .unwrap();
}
