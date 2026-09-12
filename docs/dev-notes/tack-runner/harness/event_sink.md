# `crates/tack-runner/src/harness/event_sink.rs`

Moved out of the module preamble; trim or delete freely.

Bounded, backpressured, redacted harness event streaming.

 `process.rs` bounds raw stdout/stderr *bytes*; this module bounds the
 *structured* event stream an adapter derives from a harness's output
 (one JSON object per line, a parsed tool-call, a progress update — the
 `docs/contracts/runner-v1/event-batch.request.json` `events[]` shape).
 Wiring that shape onto the wire (`PullProtocol` has no event-batch method
 yet) is future work; this module
 is the local, always-available half: what an adapter's `wait()`
 implementation accumulates before it is ever turned into a wire payload
 or a log line.

 Two independent bounds apply, matching the two different ways "memory
 bounded" can fail:

 - **Per-payload size** ([`EventSinkLimits::max_payload_bytes`], aligned
   with `limits.json`'s `event_payload_bytes_max`): one oversized event
   cannot blow the bound by itself. A payload over the cap is replaced
   with an explicit truncation marker (never a silently shortened value —
   rule 7), tracked in [`EventSinkReport::payloads_truncated`].
 - **Backpressure** ([`EventSinkLimits::channel_capacity`]): events are
   delivered over a bounded `tokio::sync::mpsc` channel. Once it is full,
   [`EventSink::push`] genuinely waits for the consumer rather than
   growing an internal buffer — see `push_backpressure_blocks_the_producer`
   below for a test that proves this is real backpressure, not merely a
   size cap on individual sends.

 A third, harder bound — [`EventSinkLimits::max_events`] — exists because
 backpressure alone only bounds the *instantaneous* buffer, not the total
 number of events a run could ever produce; a sink with nobody consuming
 it would otherwise block the producer forever rather than give a
 deterministic, testable outcome. Once the lifetime cap is reached, further
 events are counted in [`EventSinkReport::dropped_after_limit`] and never
 buffered at all.
