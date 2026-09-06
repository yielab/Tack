import { type Component, Show, createResource, createSignal } from 'solid-js';
import { Button, Field } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import {
  type CatalogSnapshot,
  VERCEL_AI_GATEWAY_SECRET_NAME,
  isLocalRunnerUnavailable,
  localRunnerApi,
} from './api';

/** Verbatim text for each `CatalogSnapshot` variant: "Catalog: N models as
 *  of <ts>" for the success case, the typed error verbatim for every
 *  other one. Never a fabricated count. */
function catalogText(catalog: CatalogSnapshot): string {
  switch (catalog.status) {
    case 'configured':
      return `Catalog: ${catalog.model_count} models as of ${new Date(catalog.checked_at).toLocaleString()}`;
    case 'not_configured':
      return 'Catalog: not configured — paste a key below.';
    case 'secret_unresolved':
      return 'Catalog: the stored key could not be read back.';
    case 'unreachable':
      return catalog.http_status
        ? `Catalog: unreachable (HTTP ${catalog.http_status}).`
        : 'Catalog: unreachable (no response).';
  }
}

/**
 * One write-only field — the Vercel AI Gateway key (ADR 0061 decision 2).
 * The value is never round-tripped: `PUT /api/local-runner/secrets/{name}`
 * answers `204` with no body, and `GET .../secrets` reports only the name
 * and when it was set. This panel is that one provider's field and nothing
 * more — every call here names `VERCEL_AI_GATEWAY_SECRET_NAME`, and the
 * catalog line below it is that provider's own. The runner can hold several
 * configured providers; offering a choice between them needs a screen built
 * for it and a response shaped per provider, neither of which is here.
 *
 * Setting a key also re-probes the catalog server-side
 * (`put_local_runner_secret`'s own doc comment,
 * `crates/tack-api/src/handlers/local_runner.rs`) — refetching `GET
 * /api/local-runner` right after save is what makes the model count appear
 * with no restart, not a second trigger from this panel.
 */
const ProviderKeyPanel: Component = () => {
  const [status, { refetch: refetchStatus }] = createResource(() => localRunnerApi.get());
  const [secrets, { refetch: refetchSecrets }] = createResource(() => localRunnerApi.listSecrets());

  const [editing, setEditing] = createSignal(false);
  const [value, setValue] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const unavailable = () => isLocalRunnerUnavailable(status.error) || isLocalRunnerUnavailable(secrets.error);
  const loadFailed = () =>
    !unavailable() && (status.error !== undefined || secrets.error !== undefined);

  const stored = () => secrets()?.data.find((s) => s.name === VERCEL_AI_GATEWAY_SECRET_NAME) ?? null;

  const refetchAll = async () => {
    await Promise.all([refetchStatus(), refetchSecrets()]);
  };

  const save = async () => {
    if (!value().trim()) return;
    setSaving(true);
    try {
      await localRunnerApi.setSecret(VERCEL_AI_GATEWAY_SECRET_NAME, value().trim());
      setValue('');
      setEditing(false);
      await refetchAll();
      toast.success('Vercel AI Gateway key saved.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save the key');
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    setSaving(true);
    try {
      await localRunnerApi.removeSecret(VERCEL_AI_GATEWAY_SECRET_NAME);
      await refetchAll();
      toast.success('Vercel AI Gateway key removed.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to remove the key');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section class="space-y-3 border-t pt-6" style={{ 'border-color': 'var(--color-border-light)' }}>
      <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        Vercel AI Gateway key
      </h2>

      <Show when={unavailable()}>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Not available from this screen — this is a remote-runner deployment. On the runner's own
          machine:
        </p>
        <pre
          class="overflow-x-auto rounded-lg border p-3 font-mono text-xs"
          style={{ 'border-color': 'var(--color-border-light)', color: 'var(--color-text-primary)' }}
        >
          tack runner secret set {VERCEL_AI_GATEWAY_SECRET_NAME}
        </pre>
        <Button variant="secondary" size="sm" onClick={() => void refetchAll()}>
          Re-check
        </Button>
      </Show>

      <Show when={loadFailed()}>
        <div class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load the current key state.{' '}
          <button type="button" class="underline" onClick={() => void refetchAll()}>
            Retry
          </button>
        </div>
      </Show>

      <Show when={!unavailable() && !loadFailed()}>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Write-only — pasted here, it is never shown or sent back again.{' '}
          <a
            href="https://vercel.com/ai-gateway"
            target="_blank"
            rel="noreferrer"
            class="underline"
          >
            Get a Vercel AI Gateway key
          </a>
        </p>

        <Show
          when={stored() && !editing()}
          fallback={
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void save();
              }}
              class="max-w-md space-y-3"
            >
              <Field
                label="API key"
                type="password"
                required
                placeholder="paste the key here"
                value={value()}
                onInput={(e) => setValue(e.currentTarget.value)}
              />
              <div class="flex gap-2">
                <Button type="submit" loading={saving()} disabled={saving() || !value().trim()}>
                  Save
                </Button>
                <Show when={stored()}>
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={saving()}
                    onClick={() => {
                      setEditing(false);
                      setValue('');
                    }}
                  >
                    Cancel
                  </Button>
                </Show>
              </div>
            </form>
          }
        >
          <div class="flex flex-wrap items-center gap-2 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            <span>
              Set {stored()!.set_at ? new Date(stored()!.set_at!).toLocaleDateString() : '(unknown date)'}
            </span>
            <button type="button" class="underline" onClick={() => setEditing(true)}>
              Replace
            </button>
            <button type="button" class="underline" disabled={saving()} onClick={() => void remove()}>
              Remove
            </button>
          </div>
        </Show>

        <Show when={!status.loading && status() !== undefined}>
          <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
            {catalogText(status()!.catalog)}
          </p>
        </Show>
      </Show>
    </section>
  );
};

export default ProviderKeyPanel;
