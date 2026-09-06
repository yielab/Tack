import { toast } from '../ui/toast';
import type { DispatchItemResponse } from './api';
import { describeDispatchOutcome, dispatchOutcomeDetail } from './format';

/**
 * Toasts a single-item dispatch outcome with a tone-appropriate toast type,
 * so "waiting approval" never reads as a plain success toast: rendering
 * waiting-approval as success is a bug, not a wording nit. Shared by the
 * item-detail dispatch control and
 * the board card menu so both toast identically rather than each mapping
 * `outcome` to a toast type on its own.
 *
 * `label` prefixes the message — pass the item title when dispatching from a
 * list context (the board card menu) where the toast isn't already anchored
 * to a single visible item the way the item-detail drawer is.
 */
export function notifyDispatchOutcome(res: DispatchItemResponse, label?: string): void {
  const desc = describeDispatchOutcome(res.outcome);
  const detail = dispatchOutcomeDetail(res);
  const message = `${label ? `${label}: ` : ''}${desc.label}${detail ? ` — ${detail}` : ''}`;

  if (res.outcome === 'blocked') toast.error(message);
  else if (res.outcome === 'waiting_approval') toast.warning(message);
  else if (res.outcome === 'dispatched') toast.success(message);
  else toast.info(message);
}
