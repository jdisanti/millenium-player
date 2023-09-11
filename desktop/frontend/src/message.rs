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

use crate::error;
use millenium_post_office::frontend::message::FrontendMessage;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ipc, js_name = postMessage)]
    fn ffi_post_message(value: &str);

    #[allow(unused)]
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(value: &str);
}

#[wasm_bindgen]
pub fn handle_message(value: JsValue) {
    match serde_wasm_bindgen::from_value(value) {
        Ok(message) => crate::handle_message(message),
        Err(err) => error!("failed to deserialize message: {err}"),
    }
}

pub fn post_message(message: &FrontendMessage) {
    let value = serde_json::to_string(&message).expect("serializable");
    ffi_post_message(&value)
}
