import { type Component, For, Show, createSignal } from 'solid-js';
import { Badge, Button, Field, Modal } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { runnersApi, type EnrollRunnerResult } from '../../../shared/execution';
import RunnerHealthCard, { type RunnerConnectionStatus } from './RunnerHealthCard';
import { formatExpiresIn, parseOptionalJsonObject } from './format';

/** One runner identity this browser session knows about — either just
 *  enrolled here, or manually revoked here after being enrolled elsewhere.
 *  Deliberately holds no secret: the raw `enrollment_token` never enters
 *  this array (see `EnrollRunnerResult`'s own doc comment in
 *  `shared/execution/api.ts` — "must display/copy it immediately and MUST
 *  NOT persist it"). Session-only (a plain signal, not `localStorage`/
 *  `sessionStorage`) — a reload starts with an empty roster, which is
 *  honest: there is no endpoint to repopulate it from (see this file's
 *  header comment). */
interface SessionRunner {
  runnerId: string;
  name: string;
  totalCapacity: number | null;
  availableCapacity: number | null;
  labels: unknown;
  connectionStatus: RunnerConnectionStatus;
  connectionReason: string;
}

const UNCONFIRMED_REASON =
  'Enrolled this session. Tack has no endpoint to read back whether the runner has connected — ' +
  'there is no GET /runners (or equivalent) route today (see this card\'s handoff, gap 1).';

/**
 * Enrollment/revocation UI for Part III runners (TODO.md III-E3). Backed by
 * real endpoints — `POST /runners/enrollment`, `POST /runners/{id}/revoke`,
 * `POST /runners/{id}/enrollment-tokens/{token_id}/revoke`
 * (`crates/tack-api/src/handlers/runner_admin.rs`) — but there is currently
 * no way to LIST existing runners (`runner_admin.rs::routes()` registers no
 * `GET /runners`, confirmed by reading the file directly, matching
 * `docs/agent-handoffs/part-iii/III-E2.md`'s gap 1). So this panel can only
 * ever show runners this browser session has itself enrolled or explicitly
 * revoked by ID — never a full roster. That limitation is stated up front,
 * not discovered by an operator staring at an empty list wondering if
 * something is broken.
 */
const EnrollmentPanel: Component = () => {
  const [sessionRunners, setSessionRunners] = createSignal<SessionRunner[]>([]);

  const [name, setName] = createSignal('');
  const [totalCapacity, setTotalCapacity] = createSignal('1');
  const [availableCapacity, setAvailableCapacity] = createSignal('1');
  const [labelsRaw, setLabelsRaw] = createSignal('');
  const [enrolling, setEnrolling] = createSignal(false);

  // The one-time token — cleared the moment the modal closes so "a
  // credential displays once only" holds structurally, not just by
  // convention (III-E3's acceptance bar).
  const [freshToken, setFreshToken] = createSignal<EnrollRunnerResult | undefined>();
  const [copied, setCopied] = createSignal(false);

  const [revokeTargetId, setRevokeTargetId] = createSignal('');
  const [revokingManual, setRevokingManual] = createSignal(false);
  const [busyRunnerIds, setBusyRunnerIds] = createSignal<Set<string>>(new Set());

  const setBusy = (id: string, busy: boolean) => {
    setBusyRunnerIds((prev) => {
      const next = new Set(prev);
      if (busy) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const submitEnroll = async (e: Event) => {
    e.preventDefault();
    const total = Number(totalCapacity());
    const available = Number(availableCapacity());
    if (!name().trim()) {
      toast.error('Runner name is required');
      return;
    }
    if (!Number.isFinite(total) || total < 0 || !Number.isFinite(available) || available < 0) {
      toast.error('Capacity must be a non-negative number');
      return;
    }
    const parsedLabels = parseOptionalJsonObject(labelsRaw(), 'Labels');
    if (!parsedLabels.ok) {
      toast.error(parsedLabels.error);
      return;
    }
    setEnrolling(true);
    try {
      const result = await runnersApi.enroll({
        name: name().trim(),
        labels: parsedLabels.value,
        total_capacity: total,
        available_capacity: available,
      });
      setFreshToken(result);
      setCopied(false);
      setSessionRunners((prev) => [
        {
          runnerId: result.runner_id,
          name: name().trim(),
          totalCapacity: total,
          availableCapacity: available,
          labels: parsedLabels.value,
          connectionStatus: 'unconfirmed',
          connectionReason: UNCONFIRMED_REASON,
        },
        ...prev,
      ]);
      setName('');
      setTotalCapacity('1');
      setAvailableCapacity('1');
      setLabelsRaw('');
      toast.success(`Enrolled "${result.runner_id}" — copy the enrollment token now, it will not be shown again.`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to enroll runner');
    } finally {
      setEnrolling(false);
    }
  };

  const closeTokenModal = () => {
    // The load-bearing line: once this fires, no component holds the raw
    // token anywhere. Re-opening any dialog afterward cannot show it again.
    setFreshToken(undefined);
    setCopied(false);
  };

  const copyToken = async () => {
    const token = freshToken();
    if (!token) return;
    try {
      await navigator.clipboard.writeText(token.enrollment_token);
      setCopied(true);
      toast.success('Enrollment token copied to clipboard');
    } catch {
      // Clipboard API can be unavailable (permissions, non-secure context,
      // test environment) — the token is still visible and selectable in
      // the modal, so this is a degraded convenience, not a dead end.
      toast.error('Could not copy automatically — select and copy the token text below');
    }
  };

  const revokeRunner = async (runner: SessionRunner) => {
    if (!confirm(`Revoke runner "${runner.name}" (${runner.runnerId})? This cannot be undone.`)) return;
    setBusy(runner.runnerId, true);
    try {
      await runnersApi.revokeRunner(runner.runnerId);
      setSessionRunners((prev) =>
        prev.map((r) =>
          r.runnerId === runner.runnerId
            ? { ...r, connectionStatus: 'unconfigured', connectionReason: 'Revoked — this runner can no longer claim work.' }
            : r,
        ),
      );
      toast.success(`Revoked "${runner.name}"`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to revoke runner');
    } finally {
      setBusy(runner.runnerId, false);
    }
  };

  const submitManualRevoke = async (e: Event) => {
    e.preventDefault();
    const id = revokeTargetId().trim();
    if (!id) return;
    if (!confirm(`Revoke runner "${id}"? This cannot be undone.`)) return;
    setRevokingManual(true);
    try {
      await runnersApi.revokeRunner(id);
      setSessionRunners((prev) => {
        const existing = prev.find((r) => r.runnerId === id);
        if (existing) {
          return prev.map((r) =>
            r.runnerId === id
              ? { ...r, connectionStatus: 'unconfigured', connectionReason: 'Revoked — this runner can no longer claim work.' }
              : r,
          );
        }
        return [
          {
            runnerId: id,
            name: id,
            totalCapacity: null,
            availableCapacity: null,
            labels: {},
            connectionStatus: 'unconfigured',
            connectionReason: 'Revoked — this runner can no longer claim work.',
          },
          ...prev,
        ];
      });
      toast.success(`Revoked "${id}"`);
      setRevokeTargetId('');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to revoke runner');
    } finally {
      setRevokingManual(false);
    }
  };

  return (
    <div class="space-y-5">
      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        Enroll issues a one-time token a <code class="font-mono">tack-runner</code> process exchanges for a
        durable credential. Tack has no endpoint yet to list existing runners, so this page only shows
        runners enrolled or revoked from this browser this session — see{' '}
        <code class="font-mono">docs/agent-handoffs/part-iii/III-E3.md</code> for the requested{' '}
        <code class="font-mono">GET /runners</code> endpoint.
      </p>

      {/* Enroll form */}
      <form onSubmit={(e) => void submitEnroll(e)} class="max-w-md space-y-3 rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Enroll a runner
        </h3>
        <Field
          label="Name"
          required
          placeholder="laptop-runner-1"
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
        />
        <div class="grid grid-cols-2 gap-3">
          <Field
            label="Total capacity"
            type="number"
            min="0"
            required
            value={totalCapacity()}
            onInput={(e) => setTotalCapacity(e.currentTarget.value)}
            hint="Concurrent attempts this runner can run."
          />
          <Field
            label="Available capacity"
            type="number"
            min="0"
            required
            value={availableCapacity()}
            onInput={(e) => setAvailableCapacity(e.currentTarget.value)}
            hint="Usually equal to total at enrollment."
          />
        </div>
        <Field
          label="Labels (JSON object, optional)"
          placeholder='{"region":"us-east"}'
          value={labelsRaw()}
          onInput={(e) => setLabelsRaw(e.currentTarget.value)}
          hint="A flat string map the scheduler can filter on."
        />
        <Button type="submit" loading={enrolling()} disabled={enrolling() || !name().trim()}>
          Enroll
        </Button>
      </form>

      {/* Session-local roster */}
      <div class="space-y-2">
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Runners enrolled or revoked this session
        </h3>
        <Show
          when={sessionRunners().length > 0}
          fallback={
            <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
              None yet — enroll a runner above, or revoke one by ID below.
            </p>
          }
        >
          <div class="space-y-2">
            <For each={sessionRunners()}>
              {(runner) => (
                <div>
                  <RunnerHealthCard
                    name={runner.name}
                    runnerId={runner.runnerId}
                    connectionStatus={runner.connectionStatus}
                    connectionReason={runner.connectionReason}
                    capacity={
                      runner.totalCapacity !== null && runner.availableCapacity !== null
                        ? { total: runner.totalCapacity, available: runner.availableCapacity }
                        : null
                    }
                    labels={runner.labels}
                    capabilities={null}
                  />
                  <Show when={runner.connectionStatus !== 'unconfigured'}>
                    <div class="mt-1">
                      <Button
                        size="sm"
                        variant="danger"
                        loading={busyRunnerIds().has(runner.runnerId)}
                        disabled={busyRunnerIds().has(runner.runnerId)}
                        onClick={() => void revokeRunner(runner)}
                      >
                        Revoke runner
                      </Button>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* Manual revoke by ID — the fallback for a runner not enrolled this session */}
      <form
        onSubmit={(e) => void submitManualRevoke(e)}
        class="max-w-md space-y-3 rounded-lg border p-3"
        style={{ 'border-color': 'var(--color-border-light)' }}
      >
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Revoke a runner by ID
        </h3>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          For a runner enrolled outside this session (e.g. before a page reload, or by another operator).
        </p>
        <Field
          label="Runner ID"
          placeholder="runr_..."
          value={revokeTargetId()}
          onInput={(e) => setRevokeTargetId(e.currentTarget.value)}
        />
        <Button
          type="submit"
          variant="danger"
          loading={revokingManual()}
          disabled={revokingManual() || !revokeTargetId().trim()}
        >
          Revoke
        </Button>
      </form>

      {/* One-time token modal */}
      <Modal isOpen={freshToken() !== undefined} onClose={closeTokenModal} title="Runner enrollment token" size="md">
        <Show when={freshToken()}>
          {(token) => (
            <div class="space-y-3">
              <div class="flex items-center gap-2">
                <Badge tone="danger">Shown once</Badge>
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                  This token will never be shown again. Copy it into the runner's configuration now.
                </p>
              </div>
              <div>
                <p class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>
                  Runner ID
                </p>
                <p class="font-mono text-sm" style={{ color: 'var(--color-text-primary)' }}>
                  {token().runner_id}
                </p>
              </div>
              <div>
                <p class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>
                  Enrollment token
                </p>
                <p
                  class="break-all rounded border p-2 font-mono text-sm select-all"
                  style={{ 'border-color': 'var(--color-border-light)', color: 'var(--color-text-primary)' }}
                >
                  {token().enrollment_token}
                </p>
              </div>
              <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                {formatExpiresIn(token().expires_at)} — set{' '}
                <code class="font-mono">TACK_RUNNER_ENROLLMENT_TOKEN</code> on the runner before then.
              </p>
              <div class="flex gap-2">
                <Button onClick={() => void copyToken()}>{copied() ? 'Copied' : 'Copy token'}</Button>
                <Button variant="secondary" onClick={closeTokenModal}>
                  I've copied it — close
                </Button>
              </div>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
};

export default EnrollmentPanel;
