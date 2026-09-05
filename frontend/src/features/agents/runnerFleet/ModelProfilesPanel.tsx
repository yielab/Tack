import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, EmptyState, Field, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { modelProfilesApi, type ModelProfileSummary } from '../../../shared/execution';

/**
 * Create/list UI for `model_profiles` (`GET`/`POST /model-profiles`) —
 * named `model_provider`/`model_id` pairs an execution request can
 * reference. Model ids are opaque (`shared/execution/types.ts`'s
 * `ModelCombination` doc comment: never parsed or split); this panel treats
 * `model_id` as a plain string field for the same reason.
 */
const ModelProfilesPanel: Component = () => {
  const [profiles, { refetch, mutate }] = createResource(() => modelProfilesApi.list());

  const [showForm, setShowForm] = createSignal(false);
  const [name, setName] = createSignal('');
  const [modelProvider, setModelProvider] = createSignal('');
  const [modelId, setModelId] = createSignal('');
  const [configReference, setConfigReference] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const rows = (): ModelProfileSummary[] => profiles()?.data.data ?? [];

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !modelProvider().trim() || !modelId().trim()) return;
    setSaving(true);
    try {
      const created = await modelProfilesApi.create({
        name: name().trim(),
        model_provider: modelProvider().trim(),
        model_id: modelId().trim(),
        config_reference: configReference().trim() || null,
      });
      toast.success(`Created model profile "${created.name}"`);
      mutate((prev) =>
        prev
          ? {
              ...prev,
              data: {
                ...prev.data,
                data: [
                  ...prev.data.data,
                  {
                    model_profile_id: created.model_profile_id,
                    name: created.name,
                    model_provider: created.model_provider,
                    model_id: created.model_id,
                    config_reference: configReference().trim() || null,
                    enabled: true,
                  },
                ],
              },
            }
          : prev,
      );
      setName('');
      setModelProvider('');
      setModelId('');
      setConfigReference('');
      setShowForm(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create model profile');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="space-y-4">
      <Show when={profiles.loading}>
        <Skeleton height="60px" />
      </Show>

      <Show when={!profiles.loading && profiles.error === undefined}>
        <Show
          when={rows().length > 0}
          fallback={
            <EmptyState
              icon="🧩"
              title="No model profiles yet"
              description="A named model_provider/model_id pair an execution request can reference."
            />
          }
        >
          <ul class="space-y-2">
            <For each={rows()}>
              {(profile) => (
                <li class="rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>
                      {profile.name}
                    </span>
                    <span class="font-mono text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                      {profile.model_provider} / {profile.model_id}
                    </span>
                    <Badge tone={profile.enabled ? 'success' : 'neutral'}>
                      {profile.enabled ? 'enabled' : 'disabled'}
                    </Badge>
                  </div>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show when={profiles.error !== undefined}>
        <div class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load model profiles.{' '}
          <button type="button" class="underline" onClick={() => void refetch()}>
            Retry
          </button>
        </div>
      </Show>

      <Show
        when={showForm()}
        fallback={
          <Button variant="secondary" size="sm" onClick={() => setShowForm(true)}>
            + Create model profile
          </Button>
        }
      >
        <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-3 rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
          <Field label="Name" required placeholder="sonnet-default" value={name()} onInput={(e) => setName(e.currentTarget.value)} />
          <Field
            label="Model provider"
            required
            placeholder="anthropic"
            value={modelProvider()}
            onInput={(e) => setModelProvider(e.currentTarget.value)}
          />
          <Field
            label="Model ID"
            required
            placeholder="claude-sonnet-5"
            value={modelId()}
            onInput={(e) => setModelId(e.currentTarget.value)}
            hint="Opaque — never parsed or split by Tack."
          />
          <Field
            label="Config reference (optional)"
            placeholder=""
            value={configReference()}
            onInput={(e) => setConfigReference(e.currentTarget.value)}
          />
          <div class="flex gap-2">
            <Button
              type="submit"
              loading={saving()}
              disabled={saving() || !name().trim() || !modelProvider().trim() || !modelId().trim()}
            >
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

export default ModelProfilesPanel;
