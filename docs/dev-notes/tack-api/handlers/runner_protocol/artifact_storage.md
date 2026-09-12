# `crates/tack-api/src/handlers/runner_protocol/artifact_storage.rs`

Moved out of the module preamble; trim or delete freely.

Safe, streamed artifact-content storage.

 Server-side counterpart to `tack-runner`'s `harness::artifact::ArtifactStager`
 (the local half — see that module's own doc comment: "no artifact-upload
 method yet ... this module is the local half only"). This module is the
 remote half: it receives whatever bytes a runner PUTs against
 `/api/runner/v1/attempts/{attempt_id}/artifacts/{artifact_id}/content`
 and commits them to `TACK_STORAGE_DIR` only after they are proven to
 match the manifest's declared `size_bytes`/`sha256` — a mismatch of
 either kind stages nothing (no blob, no `content_reference`).

 Three properties this module exists to guarantee, all proved by its own
 test suite below:

 - **Bounded memory.** [`ArtifactStorage::store_streaming`] never holds
   more than one chunk plus a running SHA-256 state at a time — it writes
   and hashes each chunk as it arrives and drops it, rather than
   collecting the stream into a `Vec<Bytes>` first. `store_streaming`
   aborts (deletes its temp file, returns
   [`ArtifactContentError::OversizeStream`]) the instant more bytes have
   arrived than the manifest declared — this is what defeats a
   "compression bomb"-style attack (a small claimed size, an arbitrarily
   large or unbounded actual body): the cap is enforced *while consuming*
   the stream, never after fully materializing it.
 - **No path traversal, no symlink escape.** `attempt_id` and
   `artifact_id` are both runner-supplied, opaque strings this module must
   never trust as literal path components. Every path is built from a
   hex-encoding of the id (mirroring `tack-runner`'s own `encode_id`
   convention in `harness/artifact.rs`) — a `..`, `/`, or NUL byte in an id
   becomes two harmless hex digits, so traversal via id content is
   structurally impossible, not merely rejected by a string check. Every
   temp file is opened with `create_new(true)` (refuses to follow an
   existing symlink or overwrite an existing file), and every directory is
   canonicalized and checked to still be contained within the
   canonicalized storage root before any write — defeating a
   symlink-swapped attempt directory, not just a string pattern.
 - **Checksum/size mismatch stages nothing.** The temp file is only ever
   renamed into its final, content-addressed location after both checks
   pass; any failure path deletes the temp file and returns before the
   caller ever calls `set_execution_artifact_content_reference`.
