import { describe, it, expect, vi, afterEach } from 'vitest';
import { ApiError } from '../../shared/api/client';
import { economicsApi, isOrchDisabled } from './api';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('economicsApi.summary', () => {
  it('GETs /economics/summary with no body', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(
          JSON.stringify({
            generated_at: '2026-08-05T00:00:00Z',
            min_sample_size: 5,
            events_retention_days: 90,
            overall: { key: 'overall', completed_item_count: 0 },
            by_project_type: [],
            by_item_type: [],
          }),
          { status: 200 },
        ),
      );

    const res = await economicsApi.summary();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/economics/summary');
    expect((init as RequestInit | undefined)?.method ?? 'GET').toBe('GET');
    expect(res.min_sample_size).toBe(5);
  });
});

describe('economicsApi.items', () => {
  it('GETs /economics/items with no query params by default', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ rows: [], total: 0 }), { status: 200 }));

    await economicsApi.items();

    const [url] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/economics/items');
    expect(String(url)).not.toContain('?');
  });

  it('serializes project_type/item_type/limit/offset as query params', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ rows: [], total: 0 }), { status: 200 }));

    await economicsApi.items({ project_type: 'software', item_type: 'bug', limit: 10, offset: 20 });

    const [url] = fetchMock.mock.calls[0];
    const u = new URL(String(url), 'http://localhost');
    expect(u.searchParams.get('project_type')).toBe('software');
    expect(u.searchParams.get('item_type')).toBe('bug');
    expect(u.searchParams.get('limit')).toBe('10');
    expect(u.searchParams.get('offset')).toBe('20');
  });
});

describe('economicsApi.exportCsv', () => {
  // Asserts on content rather than `instanceof Blob` — the latter trips the same
  // cross-realm jsdom quirk already named among this project's known
  // pre-existing failures (`client.test.ts`'s own `requestBlob` test), which
  // is out of scope to chase down here.
  it('requests format=csv and returns the CSV body via requestBlob', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response('item_id,project_id\n', { status: 200 }));

    const blob = await economicsApi.exportCsv();

    const [url] = fetchMock.mock.calls[0];
    const u = new URL(String(url), 'http://localhost');
    expect(u.searchParams.get('format')).toBe('csv');
    expect(await blob.text()).toContain('item_id,project_id');
  });
});

describe('economicsApi.exportJson', () => {
  it('requests a generous default limit so a full export is not silently truncated', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ rows: [], total: 0 }), { status: 200 }));

    await economicsApi.exportJson();

    const [url] = fetchMock.mock.calls[0];
    const u = new URL(String(url), 'http://localhost');
    expect(Number(u.searchParams.get('limit'))).toBeGreaterThanOrEqual(20_000);
  });

  it('lets a caller-supplied limit override the default', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ rows: [], total: 0 }), { status: 200 }));

    await economicsApi.exportJson({ limit: 5 });

    const [url] = fetchMock.mock.calls[0];
    const u = new URL(String(url), 'http://localhost');
    expect(u.searchParams.get('limit')).toBe('5');
  });
});

describe('isOrchDisabled', () => {
  it('is true for the documented code (409/403) or a legacy bare 404', () => {
    expect(isOrchDisabled(new ApiError(409, 'disabled', 'orchestration_disabled'))).toBe(true);
    expect(isOrchDisabled(new ApiError(403, 'disabled', 'orchestration_disabled'))).toBe(true);
    expect(isOrchDisabled(new ApiError(404, 'not found'))).toBe(true);
    expect(isOrchDisabled(new ApiError(500, 'server error'))).toBe(false);
    expect(isOrchDisabled(new Error('network'))).toBe(false);
    expect(isOrchDisabled(undefined)).toBe(false);
  });
});
