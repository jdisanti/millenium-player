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

use log::Level;
use std::fmt::{self, Debug};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Broadcast channel for filtering subscriptions.
pub trait Channel: Copy + Clone + Debug {
    /// True if broadcast should occur on the given channel.
    fn matches(&self, other: Self) -> bool;
}

/// Special "no channels" channel for use cases that don't need channels.
#[derive(Copy, Clone, Debug)]
pub struct NoChannels;
impl Channel for NoChannels {
    fn matches(&self, _other: Self) -> bool {
        true
    }
}

/// A message that can be broadcast.
pub trait BroadcastMessage: Clone + Debug + Send {
    type Channel: Channel;

    /// Which channel this message broadcasts on.
    fn channel(&self) -> Self::Channel;

    /// True if this message is sent frequently.
    ///
    /// This is used to decide if the message should be logged or not.
    fn frequent(&self) -> bool;
}

/// A handle to a subscription that can be used to receive messages and unsubscribe.
///
/// Subscriptions are automatically unsubscribed when this handle is dropped.
pub struct BroadcastSubscription<M: BroadcastMessage + Clone> {
    broadcaster: Broadcaster<M>,
    id: SubscriberId,
    receiver: Receiver<M>,
}

impl<M: BroadcastMessage + Clone> BroadcastSubscription<M> {
    /// Receive a message.
    ///
    /// This will block until a message is available or the sender is dropped.
    pub fn recv(&self) -> Option<M> {
        self.receiver.recv().ok()
    }

    /// Receive a message with a timeout.
    ///
    /// This will block until a message is available, the sender is dropped, or the timeout is reached.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<M> {
        self.receiver.recv_timeout(timeout).ok()
    }

    /// Try to receive a message.
    ///
    /// This will immediately return `None` if there is no message available right now.
    pub fn try_recv(&self) -> Option<M> {
        self.receiver.try_recv().ok()
    }

    /// Broadcast from this subscription.
    ///
    /// This is a short-hand for `broadcaster.broadcast_from(self, subscription, message)`.
    pub fn broadcast(&self, message: M) {
        self.broadcaster.broadcast_from(self, message);
    }

    /// Ends this subscription.
    pub fn unsubscribe(&self) {
        self.broadcaster.unsubscribe(self);
    }
}

impl<M: BroadcastMessage + Clone> Drop for BroadcastSubscription<M> {
    fn drop(&mut self) {
        self.unsubscribe()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct SubscriberId(usize);

impl fmt::Display for SubscriberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

struct Subscriber<M: BroadcastMessage> {
    id: SubscriberId,
    name: &'static str,
    channel: M::Channel,
    sender: Sender<M>,
}

struct Inner<M: BroadcastMessage> {
    subscriptions: Mutex<Vec<Subscriber<M>>>,
    next_id: AtomicUsize,
}

/// Multi-producer/multi-consumer message broadcaster.
///
/// The `Broadcaster` sends messages to several subscribers, and
/// can filter these messages based on a message channel so that
/// subscribers can decide which channels they want to listen on.
///
/// Every subscriber receives every message that matches its channel.
/// That is, if subscriber A receives a message, subscriber B will also
/// receive that same message if it also matches B's channel.
///
/// The broadcaster is meant to be shared, so cloning it is cheap.
#[derive(Clone)]
pub struct Broadcaster<M: BroadcastMessage> {
    inner: Arc<Inner<M>>,
}

impl<M: BroadcastMessage> Default for Broadcaster<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: BroadcastMessage> fmt::Debug for Broadcaster<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Broadcaster<")?;
        f.write_str(std::any::type_name::<M>())?;
        f.write_str(">")
    }
}

impl<M: BroadcastMessage> Broadcaster<M> {
    /// Create a new broadcaster.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                subscriptions: Mutex::new(Vec::new()),
                next_id: AtomicUsize::new(0),
            }),
        }
    }

    /// Subscribe to this broadcaster on the given channel.
    ///
    /// The name is used to identify this subscription in logging.
    pub fn subscribe(&self, name: &'static str, channel: M::Channel) -> BroadcastSubscription<M> {
        let id = SubscriberId(
            self.inner
                .next_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        let (sender, receiver) = mpsc::channel();
        self.inner.subscriptions.lock().unwrap().push(Subscriber {
            id,
            name,
            channel,
            sender,
        });
        BroadcastSubscription {
            broadcaster: Clone::clone(self),
            id,
            receiver,
        }
    }

    /// Unsubscribe from the broadcaster.
    pub fn unsubscribe(&self, subscription: &BroadcastSubscription<M>) {
        self.unsubscribe_id(subscription.id);
    }

    fn unsubscribe_id(&self, id: SubscriberId) {
        self.inner
            .subscriptions
            .lock()
            .unwrap()
            .retain(|subscriber| subscriber.id != id);
    }

    fn do_broadcast(&self, exclude_id: Option<SubscriberId>, message: M) {
        let channel = message.channel();
        let mut n = 0;
        let dead_subscriber = {
            let mut dead = None;
            let subscriptions = self.inner.subscriptions.lock().unwrap();
            for subscriber in subscriptions.iter() {
                if exclude_id.map(|id| id == subscriber.id).unwrap_or(false) {
                    continue;
                }
                if subscriber.channel.matches(channel) {
                    if subscriber.sender.send(message.clone()).is_err() {
                        // This subscriber is dead, so remove it from the list.
                        // We'll only unsubscribe one dead subscriber at a time since most of the
                        // time there will only be one, and that's simpler than tracking a list.
                        dead = Some((subscriber.id, subscriber.name));
                    }
                    n += 1;
                }
            }
            dead
        };

        if let Some((id, name)) = dead_subscriber {
            log::warn!("removing dead subscriber \"{name}\" ({id}) from message broadcaster",);
            self.unsubscribe_id(id);
        }

        let level = if message.frequent() {
            Level::Debug
        } else {
            Level::Info
        };
        log::log!(
            level,
            "broadcasted message to {n} subscribers on {channel:?}: {message:?}"
        );
    }

    /// Broadcast a message to all subscribers excluding the one sending the message.
    #[inline]
    pub fn broadcast_from(&self, subscription: &BroadcastSubscription<M>, message: M) {
        self.do_broadcast(Some(subscription.id), message);
    }

    /// Broadcast a message to all the subscribers.
    #[inline]
    pub fn broadcast(&self, message: M) {
        self.do_broadcast(None, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum TestChannel {
        All,
        A,
        B,
    }

    impl Channel for TestChannel {
        fn matches(&self, other: Self) -> bool {
            match (*self, other) {
                (Self::All, _) | (_, Self::All) => true,
                _ => *self == other,
            }
        }
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum TestMessage {
        A,
        B,
        C,
    }

    impl BroadcastMessage for TestMessage {
        type Channel = TestChannel;

        fn channel(&self) -> Self::Channel {
            match self {
                Self::A => Self::Channel::A,
                Self::B => Self::Channel::B,
                Self::C => Self::Channel::All,
            }
        }

        fn frequent(&self) -> bool {
            false
        }
    }

    fn check_send<V: Send>(value: V) -> V {
        value
    }
    fn check_send_sync<V: Send + Sync>(value: V) -> V {
        value
    }

    #[test]
    #[ntest::timeout(500)]
    fn no_subscribers() {
        let broadcaster = check_send_sync(Broadcaster::<TestMessage>::new());
        broadcaster.broadcast(TestMessage::A);
        broadcaster.broadcast(TestMessage::B);
        broadcaster.broadcast(TestMessage::C);
    }

    #[test]
    #[ntest::timeout(500)]
    fn unsubscribe_on_drop() {
        let broadcaster = Broadcaster::<TestMessage>::new();
        assert_eq!(0, broadcaster.inner.subscriptions.lock().unwrap().len());

        let sub = check_send(broadcaster.subscribe("one", TestChannel::All));
        assert_eq!(1, broadcaster.inner.subscriptions.lock().unwrap().len());

        broadcaster.broadcast(TestMessage::A);
        assert_eq!(TestMessage::A, sub.recv().unwrap());

        drop(sub);
        assert_eq!(0, broadcaster.inner.subscriptions.lock().unwrap().len());
    }

    #[test]
    #[ntest::timeout(500)]
    fn multiple_subscribers() {
        let broadcaster = Broadcaster::<TestMessage>::new();

        let sub1 = broadcaster.subscribe("one", TestChannel::All);
        let sub2 = broadcaster.subscribe("two", TestChannel::All);

        for &message in &[TestMessage::A, TestMessage::B, TestMessage::C] {
            broadcaster.broadcast(message);
            assert_eq!(message, sub1.recv().unwrap());
            assert_eq!(message, sub2.recv().unwrap());
        }
    }

    #[test]
    #[ntest::timeout(500)]
    fn channel_filtering() {
        let broadcaster = Broadcaster::<TestMessage>::new();

        let sub1 = broadcaster.subscribe("one", TestChannel::All);
        let sub2 = broadcaster.subscribe("two", TestChannel::A);
        let sub3 = broadcaster.subscribe("three", TestChannel::B);

        broadcaster.broadcast(TestMessage::A);
        broadcaster.broadcast(TestMessage::B);
        broadcaster.broadcast(TestMessage::C);

        assert_eq!(TestMessage::A, sub1.recv().unwrap());
        assert_eq!(TestMessage::B, sub1.recv().unwrap());
        assert_eq!(TestMessage::C, sub1.recv().unwrap());
        assert!(dbg!(sub1.try_recv()).is_none());

        assert_eq!(TestMessage::A, sub2.recv().unwrap());
        assert_eq!(TestMessage::C, sub2.recv().unwrap());
        assert!(dbg!(sub2.try_recv()).is_none());

        assert_eq!(TestMessage::B, sub3.recv().unwrap());
        assert_eq!(TestMessage::C, sub3.recv().unwrap());
        assert!(dbg!(sub3.try_recv()).is_none());
    }

    #[test]
    #[ntest::timeout(500)]
    fn subscriber_broadcasts_dont_circle_back() {
        let broadcaster = Broadcaster::<TestMessage>::new();

        let sub1 = broadcaster.subscribe("one", TestChannel::All);
        let sub2 = broadcaster.subscribe("two", TestChannel::A);
        let sub3 = broadcaster.subscribe("three", TestChannel::B);

        sub1.broadcast(TestMessage::A);
        sub1.broadcast(TestMessage::B);
        sub1.broadcast(TestMessage::C);

        assert!(dbg!(sub1.try_recv()).is_none());

        assert_eq!(TestMessage::A, sub2.recv().unwrap());
        assert_eq!(TestMessage::C, sub2.recv().unwrap());
        assert!(dbg!(sub2.try_recv()).is_none());

        assert_eq!(TestMessage::B, sub3.recv().unwrap());
        assert_eq!(TestMessage::C, sub3.recv().unwrap());
        assert!(dbg!(sub3.try_recv()).is_none());
    }
}
