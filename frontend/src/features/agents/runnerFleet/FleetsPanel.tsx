import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, EmptyState, Field, Select, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { fleetsApi, runnersApi, type FleetSummary, type RunnerSummary } from '../../../shared/execution';
import { parseOptionalJsonObject } from './format';
import { runnerStateBadge } from './RunnerHealthCard';

/**
 * Create/list UI for `agent_fleets` (`GET`/`POST /runner-fleets`,
 * `crates/tack-api/src/handlers/runner_admin.rs`) — a scheduling group of
 * runners a request can target with `{kind:"fleet", fleet_id}`
 * (`RunnerSelector`, `shared/execution/types.ts`). Distinct from
 * `../api.ts`'s `FleetRow`/`FleetEntry` (an older, per-project Docket
 * control-plane roster) — see that file's own header comment; nothing here
 * imports from it.
 *
 * No endpoint returns a fleet's roster directly — `POST`/`DELETE
 * /runner-fleets/{fleet_id}/members[/{runner_id}]` write membership, and
 * `RunnerSummary.fleet_ids` (`GET /runners`) is the read-back: a fleet's
 * roster is every runner whose `fleet_ids` contains it. This panel fetches
 * the full runner list once and filters it per fleet, so an add/remove
 * always renders the server's own roster afterward, never an optimistic
 * client-side guess.
 */
const FleetsPanel: Component = () => {
  const [fleets, { refetch, mutate }] = createResource(() => fleetsApi.list());
  const [runners, { refetch: refetchRunners }] = createResource(() => runnersApi.list());

  const [showForm, setShowForm] = createSignal(false);
  const [name, setName] = createSignal('');
  const [concurrencyLimit, setConcurrencyLimit] = createSignal('');
  const [defaultPolicyRaw, setDefaultPolicyRaw] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const [selectedRunner, setSelectedRunner] = createSignal<Record<string, string>>({});
  const [busyKey, setBusyKey] = createSignal<string | null>(null);

  const rows = (): FleetSummary[] => fleets()?.data.data ?? [];
  const allRunners = (): RunnerSummary[] => runners()?.data.data ?? [];
  const membersOf = (fleetId: string): RunnerSummary[] =>
    allRunners().filter((r) => r.fleet_ids.includes(fleetId));
  const nonMembersOf = (fleetId: string): RunnerSummary[] =>
    allRunners().filter((r) => !r.fleet_ids.includes(fleetId));

  const addMember = async (fleetId: string) => {
    const runnerId = selectedRunner()[fleetId];
    if (!runnerId) return;
    const runnerName = allRunners().find((r) => r.runner_id === runnerId)?.name ?? runnerId;
    setBusyKey(`add:${fleetId}`);
    try {
      const result = await fleetsApi.addMember(fleetId, runnerId);
      if (result.state === 'already_member') {
        toast.info(`${runnerName} is already a member of this fleet`);
      } else {
        toast.success(`Added ${runnerName} to the fleet`);
      }
      setSelectedRunner((prev) => ({ ...prev, [fleetId]: '' }));
      await refetchRunners();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to add runner to fleet');
    } finally {
      setBusyKey(null);
    }
  };

  const removeMember = async (fleetId: string, runnerId: string, runnerName: string) => {
    setBusyKey(`remove:${fleetId}:${runnerId}`);
    try {
      await fleetsApi.removeMember(fleetId, runnerId);
      toast.success(`Removed ${runnerName} from the fleet`);
      await refetchRunners();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to remove runner from fleet');
    } finally {
      setBusyKey(null);
    }
  };

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
                  <div class="mt-2">
                    <p class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>
                      Members
                    </p>
                    <Show
                      when={!runners.loading}
                      fallback={
                        <p class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                          Loading runners…
                        </p>
                      }
                    >
                      <Show
                        when={membersOf(fleet.fleet_id).length > 0}
                        fallback={
                          <p class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                            No members yet.
                          </p>
                        }
                      >
                        <ul class="mt-1 space-y-1">
                          <For each={membersOf(fleet.fleet_id)}>
                            {(runner) => (
                              <li class="flex items-center gap-2">
                                <span class="text-xs" style={{ color: 'var(--color-text-primary)' }}>
                                  {runner.name}
                                </span>
                                <Badge tone={runnerStateBadge(runner.state).tone}>
                                  {runnerStateBadge(runner.state).label}
                                </Badge>
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  class="ml-auto"
                                  loading={busyKey() === `remove:${fleet.fleet_id}:${runner.runner_id}`}
                                  disabled={busyKey() !== null}
                                  onClick={() => void removeMember(fleet.fleet_id, runner.runner_id, runner.name)}
                                >
                                  Remove
                                </Button>
                              </li>
                            )}
                          </For>
                        </ul>
                      </Show>

                      <Show
                        when={nonMembersOf(fleet.fleet_id).length > 0}
                        fallback={
                          <p class="mt-2 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                            No other runners available to add.
                          </p>
                        }
                      >
                        <div class="mt-2 flex items-end gap-2">
                          <Select
                            label="Add runner"
                            value={selectedRunner()[fleet.fleet_id] ?? ''}
                            onInput={(e) => {
                              const value = e.currentTarget.value;
                              setSelectedRunner((prev) => ({ ...prev, [fleet.fleet_id]: value }));
                            }}
                            options={[
                              { value: '', label: 'Select a runner' },
                              ...nonMembersOf(fleet.fleet_id).map((r) => ({ value: r.runner_id, label: r.name })),
                            ]}
                          />
                          <Button
                            size="sm"
                            loading={busyKey() === `add:${fleet.fleet_id}`}
                            disabled={!selectedRunner()[fleet.fleet_id] || busyKey() !== null}
                            onClick={() => void addMember(fleet.fleet_id)}
                          >
                            Add
                          </Button>
                        </div>
                      </Show>
                    </Show>
                  </div>
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
