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

/// Macro for extracting the target value from an [`Event`](web_sys::Event) or [`InputEvent`](web_sys::InputEvent).
///
/// Equivalent to the following Typescript:
/// ```typescript
/// (event: Event) => {
///     return event.target.value;
/// }
/// ```
#[macro_export]
macro_rules! input_value {
    ($event:expr) => {{
        use wasm_bindgen::JsCast;
        use web_sys::HtmlInputElement;

        let event = $event;
        let target = event.target().expect("event will have a target");
        let input = target
            .dyn_into::<HtmlInputElement>()
            .expect("target is an HtmlInputElement");
        input.value()
    }};
}
