//! Bounded, backpressured, redacted harness event streaming.
//!
//! `process.rs` bounds raw stdout/stderr *bytes*; this module bounds the
//! *structured* event stream an adapter derives from a harness's output
//! (one JSON object per line, a parsed tool-call, a progress update — the
//! `docs/contracts/runner-v1/event-batch.request.json` `events[]` shape).
//! Wiring that shape onto the wire (`PullProtocol` has no event-batch method
//! yet) is future work; this module
//! is the local, always-available half: what an adapter's `wait()`
//! implementation accumulates before it is ever turned into a wire payload
//! or a log line.
//!
//! Two independent bounds apply, matching the two different ways "memory
//! bounded" can fail:
//!
//! - **Per-payload size** ([`EventSinkLimits::max_payload_bytes`], aligned
//!   with `limits.json`'s `event_payload_bytes_max`): one oversized event
//!   cannot blow the bound by itself. A payload over the cap is replaced
//!   with an explicit truncation marker (never a silently shortened value —
//!   rule 7), tracked in [`EventSinkReport::payloads_truncated`].
//! - **Backpressure** ([`EventSinkLimits::channel_capacity`]): events are
//!   delivered over a bounded `tokio::sync::mpsc` channel. Once it is full,
//!   [`EventSink::push`] genuinely waits for the consumer rather than
//!   growing an internal buffer — see `push_backpressure_blocks_the_producer`
//!   below for a test that proves this is real backpressure, not merely a
//!   size cap on individual sends.
//!
//! A third, harder bound — [`EventSinkLimits::max_events`] — exists because
//! backpressure alone only bounds the *instantaneous* buffer, not the total
//! number of events a run could ever produce; a sink with nobody consuming
//! it would otherwise block the producer forever rather than give a
//! deterministic, testable outcome. Once the lifetime cap is reached, further
//! events are counted in [`EventSinkReport::dropped_after_limit`] and never
//! buffered at all.

use tokio::sync::mpsc;

use super::redact::SecretMaterial;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSource {
    Harness,
    Runner,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HarnessEvent {
    pub sequence: u64,
    pub source: EventSource,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub kind: String,
    pub payload: serde_json::Value,
    /// True when `payload` was replaced with a truncation marker because it
    /// exceeded `max_payload_bytes`.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSinkLimits {
    pub channel_capacity: usize,
    pub max_payload_bytes: usize,
    pub max_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventSinkReport {
    pub emitted: u64,
    pub payloads_truncated: u64,
    pub dropped_after_limit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Accepted,
    /// The lifetime `max_events` cap was already reached; nothing was
    /// buffered or sent for this call.
    DroppedAfterLimit,
    /// The receiver has been dropped; nothing more can ever be delivered.
    /// Distinct from `DroppedAfterLimit` so a caller can tell "the sink is
    /// full" apart from "nobody is listening anymore".
    ReceiverClosed,
}

/// The producer half. Construction always returns the paired
/// [`mpsc::Receiver`] so a sink can never exist detached from something that
/// can eventually apply backpressure.
pub struct EventSink<C> {
    sender: mpsc::Sender<HarnessEvent>,
    limits: EventSinkLimits,
    secrets: SecretMaterial,
    clock: C,
    next_sequence: u64,
    report: EventSinkReport,
}

impl<C> EventSink<C>
where
    C: crate::Clock,
{
    pub fn new(
        limits: EventSinkLimits,
        secrets: SecretMaterial,
        clock: C,
    ) -> (Self, mpsc::Receiver<HarnessEvent>) {
        let (sender, receiver) = mpsc::channel(limits.channel_capacity.max(1));
        (
            Self {
                sender,
                limits,
                secrets,
                clock,
                next_sequence: 0,
                report: EventSinkReport::default(),
            },
            receiver,
        )
    }

    /// Redacts `payload` (recursively, every string leaf), caps its
    /// serialized size, assigns the next monotonic sequence number and an
    /// `occurred_at` timestamp from the injected clock, then sends it.
    /// Awaiting this call is exactly where backpressure is felt: it only
    /// resolves once the bounded channel has room.
    pub async fn push(
        &mut self,
        source: EventSource,
        kind: impl Into<String>,
        mut payload: serde_json::Value,
    ) -> PushOutcome {
        if self.report.emitted >= self.limits.max_events {
            self.report.dropped_after_limit += 1;
            return PushOutcome::DroppedAfterLimit;
        }

        self.secrets.scrub_json(&mut payload);
        let (payload, truncated) = cap_payload(payload, self.limits.max_payload_bytes);
        if truncated {
            self.report.payloads_truncated += 1;
        }

        let event = HarnessEvent {
            sequence: self.next_sequence,
            source,
            occurred_at: chrono::DateTime::<chrono::Utc>::from(self.clock.now()),
            kind: kind.into(),
            payload,
            truncated,
        };
        self.next_sequence += 1;

        if self.sender.send(event).await.is_err() {
            return PushOutcome::ReceiverClosed;
        }
        self.report.emitted += 1;
        PushOutcome::Accepted
    }

    pub fn report(&self) -> EventSinkReport {
        self.report
    }
}

fn cap_payload(payload: serde_json::Value, max_bytes: usize) -> (serde_json::Value, bool) {
    let serialized = serde_json::to_string(&payload).unwrap_or_default();
    if serialized.len() <= max_bytes {
        return (payload, false);
    }
    let cut = serialized
        .as_bytes()
        .get(..max_bytes)
        .unwrap_or(serialized.as_bytes());
    let prefix = String::from_utf8_lossy(cut).into_owned();
    (
        serde_json::json!({
            "truncated": true,
            "original_bytes": serialized.len(),
            "text_prefix": prefix,
        }),
        true,
    )
}

#[cfg(test)]
#[path = "event_sink/tests.rs"]
mod tests;
