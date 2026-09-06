// Wire-format boundary for verified artifact download:
// `GET /executions/{request_id}/attempts/{attempt_number}/artifacts/
// {artifact_id}/content` — mounted in production at
// `crates/tack-api/src/handlers/runner_protocol/artifact_download.rs`,
// proven through the real router by `crates/tack-api/tests/wiring/artifact.rs`.
//
// `artifactsApi.list` calls the discovery route — `GET
// /executions/{request_id}/attempts/{attempt_number}/artifacts`
// (`crates/tack-api/src/handlers/attempt_lists.rs`), returning every
// manifest recorded for that attempt, oldest first — `artifact_id` is no
// longer something an operator has to already know.
//
// Two HTTP outcomes from `download` distinguish two genuinely different
// server states, and neither collapses into the other ("unmeasured is
// nullable" applied to presence, not just numbers):
//   - `404 not_found` — no artifact manifest exists under this id at all.
//   - `409 conflict` — the manifest exists, but its content has not been
//     verified (streamed + checksummed) yet; genuinely different from
//     "gone", per `artifact_download.rs`'s own doc comment. `list`'s own
//     `content_verified` field reports this same fact ahead of a download
//     attempt, from the same manifest row.

import { ApiError, apiUrl, request, requestBlob } from '../api/client';

/** One `execution_artifacts` manifest row, as returned by
 *  `artifactsApi.list`. Never carries the raw storage reference — only
 *  whether one has been committed yet (`content_verified`). */
export interface ArtifactRecord {
  artifact_id: string;
  kind: string;
  name: string;
  media_type: string | null;
  size_bytes: number;
  content_verified: boolean;
  created_at: string;
}

interface ArtifactListResponse {
  protocol_version: number;
  data: ArtifactRecord[];
}

/** Path segment builder shared by every artifact-content operation below —
 *  kept in one place so the URL shape can never drift between the fetch
 *  path and the debug/manual-open path. */
function artifactContentPath(requestId: string, attemptNumber: number, artifactId: string): string {
  return `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(
    String(attemptNumber),
  )}/artifacts/${encodeURIComponent(artifactId)}/content`;
}

export const artifactsApi = {
  /** `GET /executions/{request_id}/attempts/{attempt_number}/artifacts` —
   *  every artifact manifested for this attempt, oldest first (may be
   *  empty). Throws `ApiError` with status 404 if the request or attempt
   *  does not exist. */
  list: async (requestId: string, attemptNumber: number): Promise<ArtifactRecord[]> => {
    const res = await request<ArtifactListResponse>(
      `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(
        String(attemptNumber),
      )}/artifacts`,
    );
    return res.data;
  },
  /** Fetches the verified artifact content as a `Blob`, carrying the
   *  operator's bearer token (unlike a plain `<a href download>`, which
   *  cannot attach an `Authorization` header — a fetch+blob download is
   *  chosen over the simpler anchor-tag pattern `FilesTab.tsx` uses for
   *  attachments precisely because it needs that header). Throws `ApiError` with
   *  the real status on any non-2xx response — callers should check
   *  {@link isArtifactNotFound}/{@link isArtifactContentNotVerified} to
   *  render the two distinct failure states named above. */
  download: (requestId: string, attemptNumber: number, artifactId: string) =>
    requestBlob(artifactContentPath(requestId, attemptNumber, artifactId)),
  /** The raw API URL — exposed for tests and for a future caller that wants
   *  to open the resource directly (e.g. a same-tab preview when no bearer
   *  token is configured on this deployment). Not used for the primary
   *  download path above, which needs the auth header a plain URL can't
   *  carry. */
  contentUrl: (requestId: string, attemptNumber: number, artifactId: string) =>
    apiUrl(artifactContentPath(requestId, attemptNumber, artifactId)),
};

/** No manifest exists under this `(request_id, attempt_number, artifact_id)`
 *  triple — a genuinely missing resource. */
export function isArtifactNotFound(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

/** The manifest exists, but its content has not been verified (streamed +
 *  checksummed) yet — distinct from "gone"; a caller should offer to retry
 *  rather than treat this as permanent. */
export function isArtifactContentNotVerified(err: unknown): boolean {
  return err instanceof ApiError && err.status === 409;
}
