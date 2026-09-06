import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, EmptyState } from '../ui';
import { artifactsApi, isArtifactContentNotVerified, isArtifactNotFound, type ArtifactRecord } from '../execution';

export interface ArtifactDownloadPanelProps {
  requestId: string;
  attemptNumber: number;
}

type DownloadStatusKind = 'idle' | 'downloading' | 'done' | 'not_found' | 'not_verified' | 'error';

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * One manifested artifact, with its own real download action. Calls the
 * real, mounted `GET .../artifacts/{artifact_id}/content` via `fetch` +
 * `Blob` rather than a plain `<a href download>` (the pattern
 * `FilesTab.tsx` uses for ordinary attachments) — an anchor tag cannot
 * attach the `Authorization` bearer header this operator route requires,
 * and more importantly cannot report *why* a download failed: artifact
 * failure must stay visible.
 *
 * Two failure states are kept visually AND semantically distinct, matching
 * `artifact_download.rs`'s own documented distinction: a 404 (no manifest
 * exists under this id) is a different fact from a 409 (the manifest
 * exists, but its content has not been verified/streamed in yet — worth
 * retrying, not gone). `content_verified` on the manifest row itself
 * already answers this before a download is even attempted, but the button
 * stays enabled either way — the field can be stale by the time the click
 * lands, so the real 409 is still the authority, never overridden by a
 * pre-emptive guess.
 */
const ArtifactRow: Component<{ requestId: string; attemptNumber: number; artifact: ArtifactRecord }> = (props) => {
  const [status, setStatus] = createSignal<DownloadStatusKind>('idle');
  const [errorMessage, setErrorMessage] = createSignal<string | undefined>(undefined);

  const download = async () => {
    setStatus('downloading');
    setErrorMessage(undefined);
    try {
      const blob = await artifactsApi.download(props.requestId, props.attemptNumber, props.artifact.artifact_id);
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = props.artifact.name || props.artifact.artifact_id;
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      setStatus('done');
    } catch (err) {
      if (isArtifactNotFound(err)) {
        setStatus('not_found');
      } else if (isArtifactContentNotVerified(err)) {
        setStatus('not_verified');
      } else {
        setStatus('error');
        setErrorMessage(err instanceof Error ? err.message : 'Download failed.');
      }
    }
  };

  return (
    <li
      class="space-y-2 rounded-lg border p-3"
      style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)' }}
    >
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
          {props.artifact.name}
        </span>
        <Badge tone="neutral">{props.artifact.kind}</Badge>
        <Show when={!props.artifact.content_verified}>
          <Badge tone="warning">Not verified yet</Badge>
        </Show>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          {formatSize(props.artifact.size_bytes)}
        </span>
      </div>

      <div class="flex items-center gap-2">
        <Button size="sm" variant="secondary" onClick={download} disabled={status() === 'downloading'} loading={status() === 'downloading'}>
          Download
        </Button>
      </div>

      {/* Every outcome — success and each distinct failure — is a visible,
          named state: artifact failure stays visible. */}
      <Show when={status() === 'done'}>
        <p class="text-xs" style={{ color: 'var(--color-success-700)' }}>
          Downloaded.
        </p>
      </Show>
      <Show when={status() === 'not_found'}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          No artifact with that id exists for this attempt.
        </p>
      </Show>
      <Show when={status() === 'not_verified'}>
        <p class="text-xs" style={{ color: 'var(--color-warning-700)' }}>
          This artifact's manifest exists, but its content hasn't been verified yet — try again
          shortly.
        </p>
      </Show>
      <Show when={status() === 'error'}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          {errorMessage() ?? 'Download failed.'}
        </p>
      </Show>
    </li>
  );
};

/**
 * Every artifact manifested for one attempt, reading real data from
 * `GET /executions/{request_id}/attempts/{attempt_number}/artifacts`
 * — no artifact id is ever typed by an operator.
 */
const ArtifactDownloadPanel: Component<ArtifactDownloadPanelProps> = (props) => {
  const [artifacts] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => artifactsApi.list(props.requestId, props.attemptNumber),
  );

  return (
    <div>
      <Show when={artifacts.loading}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading artifacts…
        </p>
      </Show>
      <Show when={artifacts.error}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load artifacts: {artifacts.error instanceof Error ? artifacts.error.message : 'unknown error'}
        </p>
      </Show>
      <Show when={!artifacts.loading && !artifacts.error && (artifacts() ?? []).length === 0}>
        <EmptyState title="No artifacts yet" />
      </Show>
      <Show when={!artifacts.loading && !artifacts.error && (artifacts() ?? []).length > 0}>
        <ul class="space-y-2">
          <For each={artifacts()}>
            {(artifact) => (
              <ArtifactRow requestId={props.requestId} attemptNumber={props.attemptNumber} artifact={artifact} />
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};

export default ArtifactDownloadPanel;
