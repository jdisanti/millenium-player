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

use super::{message, PlayerThreadError};
use std::any::Any;
use std::sync::{mpsc, Arc, Weak};
use std::thread;

enum StrongOrWeak<T> {
    Strong(Arc<T>),
    Weak(Weak<T>),
}
impl<T> StrongOrWeak<T> {
    fn strong(value: T) -> Self {
        Self::Strong(Arc::new(value))
    }
    fn weak(value: &StrongOrWeak<T>) -> Self {
        match value {
            Self::Strong(value) => Self::Weak(Arc::downgrade(value)),
            Self::Weak(value) => Self::Weak(value.clone()),
        }
    }
    fn upgrade(&self) -> Option<Arc<T>> {
        match self {
            Self::Strong(value) => Some(value.clone()),
            Self::Weak(value) => value.upgrade(),
        }
    }
}

pub struct PlayerThreadHandle {
    handle: StrongOrWeak<thread::JoinHandle<()>>,
    to_tx: mpsc::Sender<message::ToPlayerMessage>,
}

impl PlayerThreadHandle {
    pub(super) fn new(
        handle: thread::JoinHandle<()>,
        to_tx: mpsc::Sender<message::ToPlayerMessage>,
    ) -> Self {
        Self {
            handle: StrongOrWeak::strong(handle),
            to_tx,
        }
    }

    pub fn weak_clone(&self) -> Self {
        Self {
            handle: StrongOrWeak::weak(&self.handle),
            to_tx: self.to_tx.clone(),
        }
    }

    pub fn healthcheck(self) -> Result<Self, PlayerThreadError> {
        if let Some(handle) = self.handle.upgrade() {
            if handle.is_finished() {
                return if let Err(err) = self.join() {
                    Err(err)
                } else {
                    Err(PlayerThreadError::EarlyExit)
                };
            }
        }
        Ok(self)
    }

    pub fn send(&self, message: message::ToPlayerMessage) -> Result<(), PlayerThreadError> {
        self.to_tx.send(message)?;
        Ok(())
    }

    pub fn join(self) -> Result<(), PlayerThreadError> {
        if let StrongOrWeak::Strong(handle) = self.handle {
            assert_eq!(
                1,
                Arc::strong_count(&handle),
                "we own self and this struct never gives direct access to the handle, so strong count must be 1"
            );
            let handle = Arc::into_inner(handle).expect("checked above");
            handle.join().map_err(Self::map_join_err)?;
            Ok(())
        } else {
            panic!("attempted to join a weak handle");
        }
    }

    fn map_join_err(panic_reason: Box<dyn Any + Send>) -> PlayerThreadError {
        let panic_reason = panic_reason
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or(panic_reason.downcast_ref::<String>().cloned());
        if let Some(panic_reason) = panic_reason {
            PlayerThreadError::FailedToJoin { panic_reason }
        } else {
            PlayerThreadError::FailedToJoinNoReason
        }
    }
}
