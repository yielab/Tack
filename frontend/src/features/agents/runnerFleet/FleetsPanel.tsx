import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, EmptyState, Field, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { fleetsApi, type FleetSummary } from '../../../shared/execution';
import { parseOptionalJsonObject } from './format';

/**
 * Create/list UI for `agent_fleets` (`GET`/`POST /runner-fleets`,
 * `crates/tack-api/src/handlers/runner_admin.rs`) — a scheduling group of
 * runners a request can target with `{kind:"fleet", fleet_id}`
 * (`RunnerSelector`, `shared/execution/types.ts`). Distinct from
 * `../api.ts`'s `FleetRow`/`FleetEntry` (Part II's per-project Docket
 * control-plane roster) — see that file's own header comment; nothing here
 * imports from it.
 *
 * **Membership is not readable or writable from this build.** The schema
 * already has an `agent_fleet_members` join table (migration 041,
 * `crates/tack-db/src/migrations.rs`) but `runner_admin.rs::routes()`
 * registers no route that reads or writes it — confirmed by grepping the
 * file for `member`. `FleetSummary` itself carries no roster count. Rather
 * than build a membership editor with nothing to call, this panel states
 * the gap once, next to the field it would occupy — the same "unsupported
 * is typed, unknown is explicit" discipline `RunnerHealthCard.tsx` applies
 * to health, not a client-side membership list that would silently do
 * nothing on submit.
 */
const FleetsPanel: Component = () => {
  const [fleets, { refetch, mutate }] = createResource(() => fleetsApi.list());

  const [showForm, setShowForm] = createSignal(false);
  const [name, setName] = createSignal('');
  const [concurrencyLimit, setConcurrencyLimit] = createSignal('');
  const [defaultPolicyRaw, setDefaultPolicyRaw] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const rows = (): FleetSummary[] => fleets()?.data.data ?? [];

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim()) return;
    const parsedPolicy = parseOptionalJsonObject(defaultPolicyRaw(), 'Default policy');
    if (!parsedPolicy.ok) {
      toast.error(parsedPolicy.error);
      return;
    }
    const limit = concurrencyLimit().trim() ? Number(concurrencyLimit()) : null;
    if (limit !== null && (!Number.isFinite(limit) || limit < 0)) {
      toast.error('Concurrency limit must be a non-negative number, or left blank');
      return;
    }
    setSaving(true);
    try {
      const created = await fleetsApi.create({
        name: name().trim(),
        concurrency_limit: limit,
        default_policy: parsedPolicy.value,
      });
      toast.success(`Created fleet "${created.name}"`);
      mutate((prev) =>
        prev
          ? {
              ...prev,
              data: {
                ...prev.data,
                data: [
                  ...prev.data.data,
                  {
                    fleet_id: created.fleet_id,
                    name: created.name,
                    concurrency_limit: limit,
                    default_policy: parsedPolicy.value,
                  },
                ],
              },
            }
          : prev,
      );
      setName('');
      setConcurrencyLimit('');
      setDefaultPolicyRaw('');
      setShowForm(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create fleet');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="space-y-4">
      <Show when={fleets.loading}>
        <Skeleton height="60px" />
      </Show>

      <Show when={!fleets.loading && fleets.error === undefined}>
        <Show
          when={rows().length > 0}
          fallback={<EmptyState icon="🚚" title="No fleets yet" description="A fleet is a named group a fleet-selector request can target." />}
        >
          <ul class="space-y-2">
            <For each={rows()}>
              {(fleet) => (
                <li class="rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
                  <div class="flex items-center gap-2">
                    <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>
                      {fleet.name}
                    </span>
                    <span class="font-mono text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                      {fleet.fleet_id}
                    </span>
                    <Badge tone="neutral">
                      {fleet.concurrency_limit === null ? 'no concurrency cap' : `cap ${fleet.concurrency_limit}`}
                    </Badge>
                  </div>
                  <p class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                    Membership isn't readable from this build — no route exposes{' '}
                    <code class="font-mono">agent_fleet_members</code> yet (requested in this card's handoff).
                  </p>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show when={fleets.error !== undefined}>
        <div class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load fleets.{' '}
          <button type="button" class="underline" onClick={() => void refetch()}>
            Retry
          </button>
        </div>
      </Show>

      <Show
        when={showForm()}
        fallback={
          <Button variant="secondary" size="sm" onClick={() => setShowForm(true)}>
            + Create fleet
          </Button>
        }
      >
        <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-3 rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
          <Field label="Name" required placeholder="backend-fleet" value={name()} onInput={(e) => setName(e.currentTarget.value)} />
          <Field
            label="Concurrency limit (optional)"
            type="number"
            min="0"
            placeholder="unlimited"
            value={concurrencyLimit()}
            onInput={(e) => setConcurrencyLimit(e.currentTarget.value)}
          />
          <Field
            label="Default policy (JSON object, optional)"
            placeholder="{}"
            value={defaultPolicyRaw()}
            onInput={(e) => setDefaultPolicyRaw(e.currentTarget.value)}
          />
          <div class="flex gap-2">
            <Button type="submit" loading={saving()} disabled={saving() || !name().trim()}>
              Create
            </Button>
            <Button type="button" variant="ghost" onClick={() => setShowForm(false)} disabled={saving()}>
              Cancel
            </Button>
          </div>
        </form>
      </Show>
    </div>
  );
};

export default FleetsPanel;
