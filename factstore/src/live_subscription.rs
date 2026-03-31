use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::EventRecord;

/// Receives committed event batches from the point of subscription onward.
///
/// A live subscription does not replay history. Each received item is one
/// committed append batch in committed sequence order. Dropping the handle stops
/// future delivery for that subscriber.
pub struct LiveSubscription {
    receiver: Receiver<Vec<EventRecord>>,
}

impl LiveSubscription {
    #[doc(hidden)]
    pub fn new(receiver: Receiver<Vec<EventRecord>>) -> Self {
        Self { receiver }
    }

    /// Blocks until the next committed batch is available or the subscription
    /// is closed.
    pub fn next_batch(&self) -> Result<Vec<EventRecord>, LiveSubscriptionRecvError> {
        self.receiver
            .recv()
            .map_err(|_| LiveSubscriptionRecvError::Closed)
    }

    /// Returns immediately with the next committed batch, `Empty` if no batch
    /// is ready, or `Closed` if the subscription is closed.
    pub fn try_next_batch(&self) -> Result<Vec<EventRecord>, TryLiveSubscriptionRecvError> {
        self.receiver.try_recv().map_err(|error| match error {
            TryRecvError::Empty => TryLiveSubscriptionRecvError::Empty,
            TryRecvError::Disconnected => TryLiveSubscriptionRecvError::Closed,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveSubscriptionRecvError {
    Closed,
}

impl Display for LiveSubscriptionRecvError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("live subscription is closed"),
        }
    }
}

impl Error for LiveSubscriptionRecvError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TryLiveSubscriptionRecvError {
    Empty,
    Closed,
}

impl Display for TryLiveSubscriptionRecvError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("live subscription has no committed batch ready"),
            Self::Closed => formatter.write_str("live subscription is closed"),
        }
    }
}

impl Error for TryLiveSubscriptionRecvError {}
