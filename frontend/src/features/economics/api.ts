// Wire-format boundary for the unit-economics dashboard.
//
// `GET /api/economics/summary` and `GET /api/economics/items` are real, already-built
// endpoints (`crates/tack-api/src/handlers/economics.rs`) — every type below was
// checked field-by-field against that file's DTOs, not guessed ahead of the backend
// landing (the "reconcile against a real endpoint" precedent every other feature
// directory in this cycle followed once its backend existed). If a field name ever
// drifts, this is the one file to fix — no other file in `features/economics/**`
// reads the wire response directly.

import { request, requestBlob, isOrchestrationDisabledError } from '../../shared/api/client';

// ─── Summary ────────────────────────────────────────────────────────────────

/** Average-or-raw duration figure — mutually exclusive per
 *  `handlers/economics.rs#LeadTimeStat`: exactly one of `avg_hours`/`raw_hours` is
 *  populated (or, at `sample_count === 0`, neither). */
export interface LeadTimeStat {
  sample_count: number;
  below_min_sample: boolean;
  avg_hours: number | null;
  raw_hours: number[] | null;
}

/** `handlers/economics.rs#ReworkStat`. `definition`/`truncation_note` travel on the
 *  wire so the rendered copy can never drift from what the number actually means. */
export interface ReworkStat {
  attempts_total: number;
  attempts_excluded_stale: number;
  attempts_with_rework_signal: number;
  below_min_sample: boolean;
  rate: number | null;
  definition: string;
  truncation_note: string;
}

/** One row of the summary — `"overall"`, one `project_type`, or one `item_type`.
 *  `handlers/economics.rs#EconomicsSlice`. */
export interface EconomicsSlice {
  key: string;
  completed_item_count: number;
  agent_completed_count: number;
  human_completed_count: number;
  tokens_in: number;
  tokens_out: number;
  cost_usd_estimated: number | null;
  pricing_snapshot_at: string | null;
  cost_usd_estimated_per_item: number | null;
  agent_lead_time: LeadTimeStat;
  human_lead_time: LeadTimeStat;
  lead_time_selection_bias_note: string;
  rework: ReworkStat;
}

export interface EconomicsSummaryResponse {
  generated_at: string;
  min_sample_size: number;
  events_retention_days: number;
  overall: EconomicsSlice;
  by_project_type: EconomicsSlice[];
  by_item_type: EconomicsSlice[];
}

// ─── Items ──────────────────────────────────────────────────────────────────

export type EconomicsPopulation = 'agent' | 'human';

/** `handlers/economics.rs#EconomicsItemResponse`. */
export interface EconomicsItemRow {
  item_id: string;
  project_id: string;
  project_type: string;
  item_type: string;
  title: string;
  status: string;
  population: EconomicsPopulation;
  started_at: string | null;
  completed_at: string | null;
  first_dispatched_at: string | null;
  attempt_count: number;
  tokens_in: number;
  tokens_out: number;
  cost_usd_estimated: number | null;
  pricing_snapshot_at: string | null;
  lead_time_hours: number | null;
  rework_applicable: boolean;
  rework_data_reliable: boolean;
  rework_signal: boolean;
}

export interface EconomicsItemsResponse {
  rows: EconomicsItemRow[];
  total: number;
}

export interface EconomicsItemsQuery {
  project_type?: string;
  item_type?: string;
  limit?: number;
  offset?: number;
}

function itemsQueryString(query: EconomicsItemsQuery & { format?: 'json' | 'csv' }): string {
  const params = new URLSearchParams();
  if (query.project_type) params.set('project_type', query.project_type);
  if (query.item_type) params.set('item_type', query.item_type);
  if (query.limit != null) params.set('limit', String(query.limit));
  if (query.offset != null) params.set('offset', String(query.offset));
  if (query.format) params.set('format', query.format);
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

/** True when the request failed because orchestration is disabled
 *  server-side — distinct from a 200 with zero completed items (enabled,
 *  nothing to show yet) and from any other failure. Delegates to
 *  `shared/api/client.ts#isOrchestrationDisabledError`;
 *  kept as its own export so every existing caller (`EconomicsPage.tsx`)
 *  keeps working unchanged. */
export function isOrchDisabled(err: unknown): boolean {
  return isOrchestrationDisabledError(err);
}

export const economicsApi = {
  summary: () => request<EconomicsSummaryResponse>('/economics/summary'),

  items: (query: EconomicsItemsQuery = {}) =>
    request<EconomicsItemsResponse>(`/economics/items${itemsQueryString(query)}`),

  exportCsv: (query: EconomicsItemsQuery = {}) =>
    requestBlob(`/economics/items${itemsQueryString({ ...query, format: 'csv' })}`),

  /** JSON export has no `Content-Disposition: attachment` on the wire (only the CSV
   *  branch of `GET /economics/items` sets one — see `handlers/economics.rs`), so
   *  this fetches the plain JSON body and lets the caller turn it into a downloadable
   *  file client-side, the same two-step `requestBlob`-or-`JSON.stringify` +
   *  `downloadBlob` split `DataPanel.tsx`'s export button already uses. Capped at a
   *  generous page size by default since a full export shouldn't silently stop at the
   *  dashboard's normal page size. */
  exportJson: (query: EconomicsItemsQuery = {}) =>
    economicsApi.items({ limit: 20_000, ...query }),
};
