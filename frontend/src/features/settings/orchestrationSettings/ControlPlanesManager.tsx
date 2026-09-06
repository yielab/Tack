import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { Badge, Button, EmptyState, Field, Select, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import CapabilityNote from '../../../shared/orch/CapabilityNote';
import { gatePause, gateModelSelection } from '../../../shared/orch/capabilities';
import { orchestrationSettingsApi, type ControlPlaneDetail } from './api';
import { elapsedSince, HEALTH_LABEL, HEALTH_TONE, healthExplanation } from './format';

export interface ControlPlanesManagerProps {
  /** The reconciler's poll interval, for accurate "health updates within
   *  ~Ns" copy — surfaced by `GET /api/settings/orchestration` and passed
   *  down rather than re-fetched here. */
  pollSecs: number;
  /** Called after a create/delete so the parent (`OrchestrationSettingsSection`)
   *  can refresh its own `control_plane_count`. */
  onChanged?: () => void;
}

/**
 * Step 2 of the guided setup: register and manage control planes.
 * `POST/GET/PATCH/DELETE /api/control-planes`
 * have been reachable, but before this the only
 * UI that ever called past a plain `GET` was `LinkForm.tsx`'s picker —
 * registering one meant a `curl POST /api/control-planes` (Fleet's own
 * pre-existing empty state literally said so). This is the first real form
 * for it.
 *
 * **No synchronous "test connection" endpoint exists.** docket's HTTP
 * surface (confirmed by reading `serve.py` directly) has
 * nothing that lets Tack probe a URL+token pair on demand — the only real
 * connectivity signal is the reconciler's own poll, which starts on the
 * very next tick after registration. So "testing" here means something
 * honest rather than a fake instant checkmark: right after registering, the
 * new row polls `GET /control-planes/{id}` every few seconds and shows the
 * real `health` value as it moves off `"unknown"` — a genuine background
 * test, not a synchronous one. A manual "Check now" button does the same
 * one-shot refetch for any existing row.
 */
const ControlPlanesManager: Component<ControlPlanesManagerProps> = (props) => {
  const [planes, { refetch, mutate }] = createResource(() =>
    orchestrationSettingsApi.listControlPlanes(),
  );

  const [showForm, setShowForm] = createSignal(false);
  const [name, setName] = createSignal('');
  const [kind, setKind] = createSignal('docket');
  const [baseUrl, setBaseUrl] = createSignal('');
  const [token, setToken] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  /** IDs currently being polled for a first health reading, so their row can
   *  show "Testing connection…" instead of the ordinary "not yet connected"
   *  copy immediately after creation. */
  const [testing, setTesting] = createSignal<Set<string>>(new Set());
  const [checking, setChecking] = createSignal<Set<string>>(new Set());

  const withFlag = (
    set: () => Set<string>,
    setSet: (s: Set<string>) => void,
    id: string,
    on: boolean,
  ) => {
    const next = new Set(set());
    if (on) next.add(id);
    else next.delete(id);
    setSet(next);
  };

  /** Poll a freshly-created plane's health a handful of times (roughly
   *  covering one reconciler tick, from `pollSecs`) so the operator sees a
   *  real result without having to manually refresh. Gives up quietly after
   *  the last attempt — the row's "Check now" button and the reconciler
   *  itself both keep working regardless. */
  const pollForFirstHealth = async (id: string) => {
    withFlag(testing, setTesting, id, true);
    const attempts = 5;
    const delayMs = Math.max(2000, Math.min(props.pollSecs * 1000, 8000));
    try {
      for (let i = 0; i < attempts; i++) {
        await new Promise((r) => setTimeout(r, delayMs));
        try {
          const updated = await orchestrationSettingsApi.getControlPlane(id);
          mutate((prev) => prev?.map((p) => (p.id === id ? updated : p)));
          if (updated.health !== 'unknown') return;
        } catch {
          // Row may have been deleted mid-poll, or a transient failure —
          // either way, stop polling rather than loop on a dead ID.
          return;
        }
      }
    } finally {
      withFlag(testing, setTesting, id, false);
    }
  };

  const checkNow = async (id: string) => {
    withFlag(checking, setChecking, id, true);
    try {
      const updated = await orchestrationSettingsApi.getControlPlane(id);
      mutate((prev) => prev?.map((p) => (p.id === id ? updated : p)));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not check this control plane');
    } finally {
      withFlag(checking, setChecking, id, false);
    }
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !baseUrl().trim()) return;
    setSaving(true);
    try {
      const created = await orchestrationSettingsApi.createControlPlane({
        name: name().trim(),
        kind: kind().trim() || undefined,
        base_url: baseUrl().trim(),
        token: token().trim() || undefined,
      });
      toast.success(`Registered "${created.name}" — testing connection…`);
      mutate((prev) => [...(prev ?? []), created]);
      setName('');
      setBaseUrl('');
      setToken('');
      setKind('docket');
      setShowForm(false);
      props.onChanged?.();
      void pollForFirstHealth(created.id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to register control plane');
    } finally {
      setSaving(false);
    }
  };

  const remove = async (cp: ControlPlaneDetail) => {
    if (!confirm(`Remove "${cp.name}"? Any project linked to it will need to be re-linked.`)) return;
    try {
      await orchestrationSettingsApi.deleteControlPlane(cp.id);
      mutate((prev) => prev?.filter((p) => p.id !== cp.id));
      toast.success(`Removed "${cp.name}"`);
      props.onChanged?.();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to remove control plane');
    }
  };

  return (
    <div class="space-y-4">
      <Show when={planes.loading}>
        <Skeleton height="80px" />
      </Show>

      <Show when={!planes.loading}>
        <Show
          when={(planes() ?? []).length > 0}
          fallback={
            <EmptyState
              icon="🛰️"
              title="No control planes registered"
              description="A control plane is a running agent-fleet runtime (e.g. docket) Tack polls for status and can dispatch work to. Register one below to continue setup."
            />
          }
        >
          <ul class="space-y-2">
            <For each={planes()}>
              {(cp) => (
                <li
                  class="flex flex-wrap items-center gap-3 rounded-lg border p-3"
                  style={{ 'border-color': 'var(--color-border-light)' }}
                >
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>
                        {cp.name}
                      </span>
                      <Badge tone="neutral">{cp.kind}</Badge>
                      <Badge tone={HEALTH_TONE[cp.health]}>{HEALTH_LABEL[cp.health]}</Badge>
                      <Show when={cp.token_set}>
                        <Badge tone="info">Token set</Badge>
                      </Show>
                    </div>
                    <div
                      class="mt-0.5 truncate font-mono text-xs"
                      style={{ color: 'var(--color-text-tertiary)' }}
                    >
                      {cp.base_url}
                    </div>
                    <p class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                      <Show
                        when={!testing().has(cp.id)}
                        fallback="Testing connection — checking in the background…"
                      >
                        {healthExplanation(cp.health, props.pollSecs)}
                        {cp.last_seen_at ? ` Last seen ${elapsedSince(cp.last_seen_at)} ago.` : ''}
                      </Show>
                    </p>
                    {/* Capability negotiation: read straight from
                        the wire payload, never from `cp.kind`.
                        `capabilities` is only `null` in the
                        `unconfigured` health case above, where there's
                        nothing to ask. Pause and model selection are the two
                        an operator configuring a plane most needs to know
                        about up front. */}
                    <Show when={cp.capabilities}>
                      {(caps) => (
                        <>
                          <CapabilityNote label="Pause" gate={gatePause(caps())} />
                          <CapabilityNote label="Model selection" gate={gateModelSelection(caps())} />
                        </>
                      )}
                    </Show>
                  </div>
                  <div class="flex items-center gap-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      loading={checking().has(cp.id)}
                      disabled={checking().has(cp.id)}
                      onClick={() => void checkNow(cp.id)}
                    >
                      Check now
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => void remove(cp)}>
                      Remove
                    </Button>
                  </div>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show
        when={showForm()}
        fallback={
          <Button variant="secondary" size="sm" onClick={() => setShowForm(true)}>
            + Register control plane
          </Button>
        }
      >
        <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-3 rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
          <Field
            label="Name"
            required
            placeholder="docket-prod"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <Field
            label="Base URL"
            required
            placeholder="https://docket.internal.example.com"
            value={baseUrl()}
            onInput={(e) => setBaseUrl(e.currentTarget.value)}
            hint="Where Tack's reconciler polls for status and sends dispatch requests."
          />
          <Field
            label="Bearer token (optional)"
            type="password"
            autocomplete="off"
            value={token()}
            onInput={(e) => setToken(e.currentTarget.value)}
            hint="Stored server-side, write-only — never shown again once saved."
          />
          <Select
            label="Kind"
            value={kind()}
            onChange={(e) => setKind(e.currentTarget.value)}
          >
            <option value="docket">docket</option>
          </Select>
          <div class="flex gap-2">
            <Button type="submit" loading={saving()} disabled={saving() || !name().trim() || !baseUrl().trim()}>
              Register
            </Button>
            <Button type="button" variant="ghost" onClick={() => setShowForm(false)} disabled={saving()}>
              Cancel
            </Button>
          </div>
        </form>
      </Show>

      <Show when={planes.error !== undefined}>
        <div class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load control planes.{' '}
          <button type="button" class="underline" onClick={() => void refetch()}>
            Retry
          </button>
        </div>
      </Show>
    </div>
  );
};

export default ControlPlanesManager;
