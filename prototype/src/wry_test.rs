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

use std::borrow::Cow;

use wry::{
    application::{
        event::{Event, StartCause, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    },
    webview::WebViewBuilder,
};

#[derive(Debug, Clone)]
struct StubUserEvent;

pub fn prototype_wry_test() {
    let _menu = muda::Menu::new();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Wry Test")
        .with_decorations(true)
        .build(&event_loop)
        .expect("failed to build window");

    #[cfg(target_os = "macos")]
    {
        use muda::{AboutMetadata, PredefinedMenuItem, Submenu};
        let app_menu = Submenu::new("Wry Test", true);
        app_menu
            .append_items(&[
                &PredefinedMenuItem::about(None, Some(AboutMetadata::default())),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ])
            .unwrap();

        let window_menu = Submenu::new("Window", true);
        window_menu
            .append(&PredefinedMenuItem::minimize(None))
            .unwrap();

        _menu.append_items(&[&app_menu, &window_menu]).unwrap();

        _menu.init_for_nsapp();
        window_menu.set_as_windows_menu_for_nsapp();
    }

    let menu_channel = muda::MenuEvent::receiver();

    let webview = WebViewBuilder::new(window)
        .expect("failed to create webview")
        .with_custom_protocol("internal".into(), move |request| {
            let path = request.uri().path();
            dbg!(path);
            let response: http::Response<String> = match path {
                "/index.html" => {
                    let html = r#"
                        <html>
                        <body style="color:#FFF;background-color:rgba(1, 1, 1, 1);">
                            <h1>Wry Test</h1>
                            <button onclick="window.ipc.postMessage('test IPC message')">Test IPC</button>
                            <div id="testnum"></div>
                            <script>
                                function set_number(number) {
                                    document.getElementById("testnum").innerHTML = number;
                                }
                            </script>
                        </body>
                        </html>
                        "#;
                    http::Response::builder()
                        .status(200)
                        .body(html.into())
                        .unwrap()
                }
                _ => http::Response::builder()
                    .status(404)
                    .body("Not Found".into())
                    .unwrap(),
            };
            Ok(response.map(|b| Cow::Owned(b.into_bytes())))
        })
        .with_ipc_handler(|_window, message| {
            dbg!(message);
        })
        .with_url("internal://localhost/index.html")
        .expect("failed to load asset")
        .with_file_drop_handler(|_window, event| {
            dbg!(event);
            true
        })
        .with_transparent(true)
        .with_visible(false)
        .build()
        .expect("failed to build webview");

    let mut test_number = 0;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        test_number += 1;
        webview
            .evaluate_script(&format!("set_number({test_number})"))
            .expect("failed to dispatch script");

        match event {
            Event::NewEvents(StartCause::Init) => println!("Wry has started!"),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => (),
        }

        if let Ok(menu_event) = menu_channel.try_recv() {
            dbg!(menu_event);
        }
    });
}
