//! Cross-thread asynchronous message channel between background Tokio tasks and the UI thread.
//!
//! Implements the single-threaded UI confinement boundary specified in **RFC-0001 §3.1**:
//! - [`UiSender`]: `Clone + Send + Sync` handle wrapping an unbounded or bounded Tokio channel.
//! - [`UiReceiver`]: UI-thread receiver draining asynchronous component messages.
//! - [`UiMessage`]: Generational target [`NodeId`] and boxed, thread-safe message payload.

use crate::id::NodeId;
use crate::runtime::arena::{Arena, NodeFlags};

use std::any::Any;
use std::cell::Cell;
use std::fmt;
use tokio::sync::mpsc::error::TryRecvError;

/// A type-erased message targeted to a component instance originating from a background task or thread.
pub struct UiMessage {
    /// Generational identifier of the target component in the arena.
    pub target: NodeId,
    /// Boxed component message payload.
    pub message: Box<dyn Any + Send>,
}

impl fmt::Debug for UiMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiMessage")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

/// Error returned when sending a message through [`UiSender`] fails.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum UiSendError {
    /// The channel receiver was dropped, meaning the UI thread is no longer listening.
    ChannelClosed,
    /// A bounded channel has reached its capacity limit.
    ChannelFull,
}

impl fmt::Display for UiSendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChannelClosed => write!(f, "UI channel closed: receiver dropped"),
            Self::ChannelFull => write!(f, "UI channel full: bounded capacity reached"),
        }
    }
}

impl std::error::Error for UiSendError {}

/// Thread-safe sender delivering asynchronous action messages to the UI thread.
///
/// Implements [`Clone`], [`Send`], and [`Sync`], wrapping an underlying Tokio
/// [`tokio::sync::mpsc`] channel.
#[derive(Clone, Debug)]
pub struct UiSender {
    inner: UiSenderInner,
}

#[derive(Clone, Debug)]
enum UiSenderInner {
    Unbounded(tokio::sync::mpsc::UnboundedSender<UiMessage>),
    Bounded(tokio::sync::mpsc::Sender<UiMessage>),
}

/// Receiver half of the cross-thread UI message channel, owned by the UI thread.
#[derive(Debug)]
pub struct UiReceiver {
    inner: UiReceiverInner,
}

#[derive(Debug)]
enum UiReceiverInner {
    Unbounded(tokio::sync::mpsc::UnboundedReceiver<UiMessage>),
    Bounded(tokio::sync::mpsc::Receiver<UiMessage>),
}

impl UiSender {
    /// Creates a new unbounded [`UiSender`] and [`UiReceiver`] pair.
    #[must_use]
    pub fn unbounded() -> (Self, UiReceiver) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                inner: UiSenderInner::Unbounded(tx),
            },
            UiReceiver {
                inner: UiReceiverInner::Unbounded(rx),
            },
        )
    }

    /// Creates a new bounded [`UiSender`] and [`UiReceiver`] pair with the specified capacity.
    #[must_use]
    pub fn bounded(capacity: usize) -> (Self, UiReceiver) {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        (
            Self {
                inner: UiSenderInner::Bounded(tx),
            },
            UiReceiver {
                inner: UiReceiverInner::Bounded(rx),
            },
        )
    }

    /// Creates a [`UiSender`] from an existing [`tokio::sync::mpsc::UnboundedSender`].
    #[must_use]
    pub fn new_unbounded(sender: tokio::sync::mpsc::UnboundedSender<UiMessage>) -> Self {
        Self {
            inner: UiSenderInner::Unbounded(sender),
        }
    }

    /// Creates a [`UiSender`] from an existing bounded [`tokio::sync::mpsc::Sender`].
    #[must_use]
    pub fn new_bounded(sender: tokio::sync::mpsc::Sender<UiMessage>) -> Self {
        Self {
            inner: UiSenderInner::Bounded(sender),
        }
    }

    /// Sends a pre-boxed message to the target component instance without blocking.
    pub fn send(&self, target: NodeId, message: Box<dyn Any + Send>) -> Result<(), UiSendError> {
        match &self.inner {
            UiSenderInner::Unbounded(tx) => tx
                .send(UiMessage { target, message })
                .map_err(|_| UiSendError::ChannelClosed),
            UiSenderInner::Bounded(tx) => {
                tx.try_send(UiMessage { target, message })
                    .map_err(|e| match e {
                        tokio::sync::mpsc::error::TrySendError::Full(_) => UiSendError::ChannelFull,
                        tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                            UiSendError::ChannelClosed
                        }
                    })
            }
        }
    }

    /// Asynchronously sends a pre-boxed message to the target component instance.
    pub async fn send_async(
        &self,
        target: NodeId,
        message: Box<dyn Any + Send>,
    ) -> Result<(), UiSendError> {
        match &self.inner {
            UiSenderInner::Unbounded(tx) => tx
                .send(UiMessage { target, message })
                .map_err(|_| UiSendError::ChannelClosed),
            UiSenderInner::Bounded(tx) => tx
                .send(UiMessage { target, message })
                .await
                .map_err(|_| UiSendError::ChannelClosed),
        }
    }

    /// Boxes and sends a typed message to the target component instance without blocking.
    #[inline]
    pub fn send_typed<M: Any + Send + 'static>(
        &self,
        target: NodeId,
        message: M,
    ) -> Result<(), UiSendError> {
        self.send(target, Box::new(message))
    }

    /// Boxes and asynchronously sends a typed message to the target component instance.
    #[inline]
    pub async fn send_typed_async<M: Any + Send + 'static>(
        &self,
        target: NodeId,
        message: M,
    ) -> Result<(), UiSendError> {
        self.send_async(target, Box::new(message)).await
    }
}

impl UiReceiver {
    /// Attempts to receive a message without blocking.
    pub fn try_recv(&mut self) -> Result<UiMessage, TryRecvError> {
        match &mut self.inner {
            UiReceiverInner::Unbounded(rx) => rx.try_recv(),
            UiReceiverInner::Bounded(rx) => rx.try_recv(),
        }
    }

    /// Asynchronously waits for a message to arrive from a background task.
    pub async fn recv(&mut self) -> Option<UiMessage> {
        match &mut self.inner {
            UiReceiverInner::Unbounded(rx) => rx.recv().await,
            UiReceiverInner::Bounded(rx) => rx.recv().await,
        }
    }

    /// Returns `true` if the channel currently has no messages waiting to be received.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match &self.inner {
            UiReceiverInner::Unbounded(rx) => rx.is_empty(),
            UiReceiverInner::Bounded(rx) => rx.is_empty(),
        }
    }

    /// Drains all currently pending cross-thread messages into the [`Arena`].
    ///
    /// # Generational Safety
    ///
    /// If a target node has been unmounted or replaced with a new generation,
    /// [`Arena::contains`] returns `false`, causing stale messages to be safely discarded.
    ///
    /// Returns the number of successfully processed messages.
    pub fn drain_messages(&mut self, arena: &mut Arena, dirty_cell: &Cell<bool>) -> usize {
        let mut processed = 0;
        while let Ok(msg) = self.try_recv() {
            if arena.contains(msg.target) {
                if let Ok(should_render) = arena.update(msg.target, msg.message, dirty_cell) {
                    if should_render.is_yes() {
                        if let Some(node) = arena.get_mut(msg.target) {
                            node.flags.insert(NodeFlags::DIRTY);
                        }
                    }
                    processed += 1;
                }
            }
        }
        processed
    }
}
