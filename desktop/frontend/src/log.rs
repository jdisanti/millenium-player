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

#[macro_export]
macro_rules! log {
    ($level:ident, $($arg:tt)*) => {{
        use millenium_post_office::frontend::message::{FrontendMessage, LogLevel};
        let message = FrontendMessage::Log { level: LogLevel::$level, message: format_args!($($arg)*).to_string() };
        $crate::message::post_message(&message);
    }};
}
#[macro_export]
macro_rules! trace { ($($arg:tt)*) => { $crate::log!(Trace, $($arg)*) }; }
#[macro_export]
macro_rules! debug { ($($arg:tt)*) => { $crate::log!(Debug, $($arg)*) }; }
#[macro_export]
macro_rules! info { ($($arg:tt)*) => { $crate::log!(Info, $($arg)*) }; }
#[macro_export]
macro_rules! warn { ($($arg:tt)*) => { $crate::log!(Warn, $($arg)*) }; }
#[macro_export]
macro_rules! error { ($($arg:tt)*) => { $crate::log!(Error, $($arg)*) }; }
