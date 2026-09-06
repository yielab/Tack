import {
  type Component,
  createResource,
  createSignal,
  For,
  Show,
  onCleanup,
} from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Badge, Button, EmptyState, Field, Modal, Skeleton } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import {
  approvalTokenStore,
  approvalsApi,
  isApprovalAlreadyDecided,
  isApprovalGone,
  isApprovalTokenRejected,
  isOrchDisabled,
  type ApprovalDecisionActionValue,
  type PendingApproval,
} from './api';
import { actionLabel, agentLabel, correlatedItemLabel, elapsedSince } from './format';

/** How often the inbox re-fetches. Approvals fail closed on timeout, so
 *  staleness has a real cost — see the module doc below for why polling,
 *  not the realtime `ApprovalPending` WebSocket event, is this page's
 *  primary freshness mechanism. */
const POLL_INTERVAL_MS = 10_000;

const COLUMNS = ['Waiting', 'Agent', 'Action', 'Item', 'Decision'];

const thStyle = {
  padding: '10px 14px',
  'text-align': 'left' as const,
  'font-size': '10.5px',
  'font-weight': 700,
  'letter-spacing': '.05em',
  'text-transform': 'uppercase' as const,
  color: 'var(--color-text-tertiary)',
  'border-bottom': '1px solid var(--color-border-light)',
  'white-space': 'nowrap' as const,
};

const tdStyle = {
  padding: '12px 14px',
  'font-size': '13px',
  'vertical-align': 'top' as const,
  'border-bottom': '1px solid var(--color-border-light)',
};

/** Shown when orchestration is off — the default state for every existing
 *  install, since orchestration is off unless an operator opts in. Links
 *  to the guided setup instead of naming an environment variable to set
 *  by hand. */
const OrchDisabledEmptyState: Component = () => {
  const navigate = useNavigate();
  return (
    <EmptyState
      icon="🔌"
      title="Agent-fleet orchestration is disabled"
      description="Turn it on to see every approval currently blocking an agent fleet, across every linked project."
      action={
        <Button onClick={() => navigate('/settings?section=orchestration')}>
          Set up orchestration
        </Button>
      }
    />
  );
};

const ErrorState: Component<{ onRetry: () => void }> = (props) => (
  <EmptyState
    icon="⚠️"
    title="Couldn't load the approvals inbox"
    description="The request to the server failed. Check your connection and try again."
    action={<Button onClick={props.onRetry}>Retry</Button>}
  />
);

const ZeroEmptyState: Component = () => (
  <EmptyState
    icon="✅"
    title="Inbox zero"
    description="No approval is currently blocking an agent fleet."
  />
);

const LoadingRows: Component = () => (
  <For each={[0, 1, 2]}>
    {() => (
      <tr>
        <For each={COLUMNS}>
          {() => (
            <td style={tdStyle}>
              <Skeleton height="14px" />
            </td>
          )}
        </For>
      </tr>
    )}
  </For>
);

/**
 * Fleet-wide approvals inbox — the surface where a human resolves the
 * approvals currently blocking an agent fleet. Oldest-requested-first:
 * docket approvals fail closed on timeout, so the longest-waiting one has
 * a real cost to leaving it unresolved. Uncorrelated approvals (docket
 * raised one but Tack couldn't attribute it to an item) are shown
 * alongside correlated ones, never filtered out — see `api.ts`'s header
 * comment.
 *
 * **Why this page polls instead of relying only on the realtime
 * `ApprovalPending` WebSocket event (card B4).** That event is delivered
 * per-project and is skipped entirely for an uncorrelated approval (B4's own
 * handoff: "a run with no Tack `item_id` ... has no project to filter into,
 * so I skip the broadcast entirely"). Since surfacing uncorrelated approvals
 * is this page's whole reason to exist, a per-project socket can't be this
 * page's primary freshness mechanism — polling is the one mechanism that
 * covers both correlated and uncorrelated rows uniformly. No WebSocket
 * subscription is opened here at all.
 *
 * **Deciding is never a single click.** Clicking Grant or Deny opens a
 * confirmation modal naming exactly what's being released (the agent, the
 * action text, and the correlated item or its absence) before the real
 * `POST /api/approvals/{token}` call fires — a decision is not reversible
 * once docket accepts it.
 */
const ApprovalsPage: Component = () => {
  const [inbox, { refetch }] = createResource(() => approvalsApi.list());

  const rows = (): PendingApproval[] => inbox()?.rows ?? [];
  const disabled = () => isOrchDisabled(inbox.error);
  const failed = () => inbox.error !== undefined && !disabled();

  const poll = setInterval(() => {
    if (!inbox.loading) void refetch();
  }, POLL_INTERVAL_MS);
  onCleanup(() => clearInterval(poll));

  // The operator's own copy of TACK_ORCH_APPROVAL_TOKEN — never sent
  // anywhere except on an actual decide call (see api.ts's approvalTokenStore
  // doc comment). Signal is session-scoped so a returning tab can decide,
  // but a long-lived browser secret is never retained.
  const [tokenInput, setTokenInput] = createSignal(approvalTokenStore.get() ?? '');

  const saveToken = () => {
    approvalTokenStore.set(tokenInput().trim() || null);
    toast.success('Approval token saved for this browser session.');
  };

  const [confirming, setConfirming] = createSignal<{
    row: PendingApproval;
    action: ApprovalDecisionActionValue;
  } | null>(null);
  const [deciding, setDeciding] = createSignal(false);

  const openConfirm = (row: PendingApproval, action: ApprovalDecisionActionValue) =>
    setConfirming({ row, action });
  const closeConfirm = () => {
    if (!deciding()) setConfirming(null);
  };

  const confirmDecision = async () => {
    const target = confirming();
    if (!target) return;
    setDeciding(true);
    try {
      const res = await approvalsApi.decide(target.row.token, target.action);
      toast.success(
        `${target.action === 'grant' ? 'Granted' : 'Denied'} — docket reports "${res.state}".`
      );
      setConfirming(null);
      await refetch();
    } catch (err) {
      if (isApprovalTokenRejected(err)) {
        toast.error(
          'Approval token rejected — check the value entered above, or that TACK_ORCH_APPROVAL_TOKEN is configured on the server.'
        );
      } else if (isApprovalAlreadyDecided(err) || isApprovalGone(err)) {
        toast.warning('This approval was already resolved elsewhere. Removing it from the inbox.');
        setConfirming(null);
        await refetch();
      } else {
        toast.error('Failed to reach the control plane — the approval is unchanged. Try again.');
      }
    } finally {
      setDeciding(false);
    }
  };

  return (
    <div>
      <div class="mb-6">
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Approvals
        </h1>
        <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Every approval currently blocking an agent fleet, oldest first — including ones Tack
          couldn't attribute to a project.
        </p>
      </div>

      {/* Always shown once the inbox itself loaded successfully — there is
          no reliable pre-check for "will granting actually work" (card G1
          retired the one that used to guess; see api.ts's
          PendingApprovalListResponse doc comment). Grant/Deny below are
          likewise always rendered; if TACK_ORCH_APPROVAL_TOKEN isn't
          configured on the server at all, or the token typed here is wrong,
          the actual decide call answers with a real 403 and
          `confirmDecision`'s catch surfaces that server-given reason. */}
      <Show when={!inbox.loading && !disabled() && !failed()}>
        <div
          class="mb-4 flex flex-wrap items-end gap-2 rounded-lg p-3"
          style={{
            border: '1px solid var(--color-border-light)',
            'background-color': 'var(--color-bg-subtle)',
          }}
        >
          <Field
            label="Your approval token"
            type="password"
            autocomplete="off"
            placeholder="TACK_ORCH_APPROVAL_TOKEN"
            value={tokenInput()}
            onInput={(e) => setTokenInput(e.currentTarget.value)}
            hint="Stored only in this browser. Required to grant or deny — never sent when just viewing this inbox."
            class="min-w-65 flex-1"
          />
          <Button variant="secondary" onClick={saveToken}>
            Save
          </Button>
        </div>
      </Show>

      <Show when={inbox.loading}>
        <div class="overflow-x-auto rounded-lg" style={{ border: '1px solid var(--color-border-light)' }}>
          <table class="w-full" style={{ 'border-collapse': 'collapse' }}>
            <thead>
              <tr>
                <For each={COLUMNS}>{(col) => <th style={thStyle}>{col}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <LoadingRows />
            </tbody>
          </table>
        </div>
      </Show>

      <Show when={!inbox.loading && disabled()}>
        <OrchDisabledEmptyState />
      </Show>

      <Show when={!inbox.loading && failed()}>
        <ErrorState onRetry={refetch} />
      </Show>

      <Show when={!inbox.loading && !disabled() && !failed() && rows().length === 0}>
        <ZeroEmptyState />
      </Show>

      <Show when={!inbox.loading && !disabled() && !failed() && rows().length > 0}>
        <div class="overflow-x-auto rounded-lg" style={{ border: '1px solid var(--color-border-light)' }}>
          <table class="w-full" style={{ 'border-collapse': 'collapse' }}>
            <caption class="sr-only">Fleet-wide pending approvals, oldest first</caption>
            <thead>
              <tr>
                <For each={COLUMNS}>{(col) => <th scope="col" style={thStyle}>{col}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <For each={rows()}>
                {(row) => (
                  <tr>
                    <td style={tdStyle}>
                      <span style={{ color: 'var(--color-text-secondary)' }}>
                        {elapsedSince(row.requested_at)}
                      </span>
                    </td>
                    <td style={tdStyle}>{agentLabel(row)}</td>
                    <td style={{ ...tdStyle, 'font-family': 'var(--font-mono)', 'max-width': '360px' }}>
                      {actionLabel(row)}
                    </td>
                    <td style={tdStyle}>
                      <Show
                        when={row.item_id}
                        fallback={<Badge tone="neutral">{correlatedItemLabel(row)}</Badge>}
                      >
                        <div>{row.item_title}</div>
                        <div class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                          {row.project_name}
                        </div>
                      </Show>
                    </td>
                    <td style={tdStyle}>
                      <div class="flex gap-2">
                        <Button
                          size="sm"
                          variant="success"
                          onClick={() => openConfirm(row, 'grant')}
                        >
                          Grant
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          onClick={() => openConfirm(row, 'deny')}
                        >
                          Deny
                        </Button>
                      </div>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      <Show when={confirming()}>
        {(target) => (
          <Modal
            isOpen
            onClose={closeConfirm}
            title={target().action === 'grant' ? 'Grant this approval?' : 'Deny this approval?'}
          >
            <div class="flex flex-col gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
              <p>
                This releases the agent that's currently waiting on this decision.{' '}
                <strong>This cannot be undone.</strong>
              </p>
              <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
                <dt style={{ color: 'var(--color-text-tertiary)' }}>Agent</dt>
                <dd>{agentLabel(target().row)}</dd>
                <dt style={{ color: 'var(--color-text-tertiary)' }}>Action</dt>
                <dd style={{ 'font-family': 'var(--font-mono)' }}>{actionLabel(target().row)}</dd>
                <dt style={{ color: 'var(--color-text-tertiary)' }}>Item</dt>
                <dd>{correlatedItemLabel(target().row)}</dd>
                <dt style={{ color: 'var(--color-text-tertiary)' }}>Waiting</dt>
                <dd>{elapsedSince(target().row.requested_at)}</dd>
              </dl>
              <div class="mt-2 flex justify-end gap-2">
                <Button variant="secondary" onClick={closeConfirm} disabled={deciding()}>
                  Cancel
                </Button>
                <Button
                  variant={target().action === 'grant' ? 'success' : 'danger'}
                  onClick={confirmDecision}
                  loading={deciding()}
                >
                  {target().action === 'grant' ? 'Grant' : 'Deny'}
                </Button>
              </div>
            </div>
          </Modal>
        )}
      </Show>
    </div>
  );
};

export default ApprovalsPage;
