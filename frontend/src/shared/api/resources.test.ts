import { describe, it, expect, vi, beforeEach } from 'vitest';
import { api, tokenStore } from './index';

// Verifies the URL + HTTP method (and body where relevant) of every resource
// method on the typed api.* client. The page handlers are thin
// wrappers over these, so asserting the contract here covers "the converted
// page calls api.* against the right endpoint".

let fetchMock: ReturnType<typeof vi.spyOn>;

function ok(body: unknown = {}): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

function lastCall() {
  const [url, init] = fetchMock.mock.calls.at(-1)!;
  return { url: url as string, method: (init?.method ?? 'GET') as string, body: init?.body, headers: init?.headers };
}

beforeEach(() => {
  tokenStore.set(null);
  fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok());
});

describe('api.projects', () => {
  it('list → GET /api/projects', async () => {
    await api.projects.list();
    expect(lastCall()).toMatchObject({ url: '/api/projects', method: 'GET' });
  });
  it('get → GET /api/projects/{id}', async () => {
    await api.projects.get('p1');
    expect(lastCall()).toMatchObject({ url: '/api/projects/p1', method: 'GET' });
  });
  it('update → PATCH /api/projects/{id}', async () => {
    await api.projects.update('p1', { name: 'x' });
    expect(lastCall()).toMatchObject({ url: '/api/projects/p1', method: 'PATCH' });
  });
});

describe('api.items', () => {
  it('list → GET /api/projects/{id}/items?page=1&per_page=200 (paginated envelope)', async () => {
    fetchMock.mockResolvedValue(ok({ data: [], total: 0, page: 1, per_page: 200 }));
    await api.items.list('p1');
    expect(lastCall()).toMatchObject({
      url: '/api/projects/p1/items?page=1&per_page=200',
      method: 'GET',
    });
  });
  it('update fetches an ETag then sends it as If-Match', async () => {
    fetchMock
      .mockResolvedValueOnce(new Response(JSON.stringify({ item: { id: 'i1' } }), {
        status: 200,
        headers: { 'Content-Type': 'application/json', ETag: '"i1-1"' },
      }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'i1' }), {
        status: 200,
        headers: { 'Content-Type': 'application/json', ETag: '"i1-2"' },
      }));
    await api.items.update('i1', { status: 'done' });
    const call = lastCall();
    expect(call).toMatchObject({ url: '/api/items/i1', method: 'PATCH' });
    expect(new Headers(call.headers).get('If-Match')).toBe('"i1-1"');
  });
});


describe('api.sprints (Sprints)', () => {
  it('list → GET /api/projects/{id}/sprints', async () => {
    await api.sprints.list('p1');
    expect(lastCall()).toMatchObject({ url: '/api/projects/p1/sprints', method: 'GET' });
  });
  it('create → POST /api/projects/{id}/sprints', async () => {
    await api.sprints.create('p1', { name: 'Sprint 1' });
    expect(lastCall()).toMatchObject({ url: '/api/projects/p1/sprints', method: 'POST' });
  });
  it('update → PATCH /api/sprints/{id} with the editable fields', async () => {
    await api.sprints.update('s1', { name: 'Renamed', goal: 'Ship it' });
    const c = lastCall();
    expect(c).toMatchObject({ url: '/api/sprints/s1', method: 'PATCH' });
    expect(JSON.parse(c.body as string)).toEqual({ name: 'Renamed', goal: 'Ship it' });
  });
  it('setStatus → PATCH /api/sprints/{id}/status with body', async () => {
    await api.sprints.setStatus('s1', 'active');
    const c = lastCall();
    expect(c).toMatchObject({ url: '/api/sprints/s1/status', method: 'PATCH' });
    expect(JSON.parse(c.body as string)).toEqual({ status: 'active' });
  });
});

describe('api.templates (Templates / TemplateCreator)', () => {
  it('list (no filter) → GET /api/templates', async () => {
    await api.templates.list();
    expect(lastCall()).toMatchObject({ url: '/api/templates', method: 'GET' });
  });
  it('list (filtered) → GET /api/templates?project_type=...', async () => {
    await api.templates.list('software');
    expect(lastCall().url).toBe('/api/templates?project_type=software');
  });
  it('create → POST /api/templates', async () => {
    await api.templates.create({ name: 't', project_type: 'software' });
    expect(lastCall()).toMatchObject({ url: '/api/templates', method: 'POST' });
  });
  it('remove → DELETE /api/templates/{id}', async () => {
    await api.templates.remove('t1');
    expect(lastCall()).toMatchObject({ url: '/api/templates/t1', method: 'DELETE' });
  });
  it('createProject → POST /api/projects/from-template/{id}', async () => {
    await api.templates.createProject('t1', { name: 'New' });
    expect(lastCall()).toMatchObject({
      url: '/api/projects/from-template/t1',
      method: 'POST',
    });
  });
});

describe('api.customFields (CustomFieldsManager)', () => {
  it('list → GET /api/projects/{id}/custom-fields', async () => {
    await api.customFields.list('p1');
    expect(lastCall()).toMatchObject({
      url: '/api/projects/p1/custom-fields',
      method: 'GET',
    });
  });
  it('create → POST /api/projects/{id}/custom-fields', async () => {
    await api.customFields.create('p1', { name: 'Client', field_type: 'text' });
    expect(lastCall()).toMatchObject({
      url: '/api/projects/p1/custom-fields',
      method: 'POST',
    });
  });
  it('update → PATCH /api/custom-fields/{id}', async () => {
    await api.customFields.update('f1', { name: 'X' });
    expect(lastCall()).toMatchObject({ url: '/api/custom-fields/f1', method: 'PATCH' });
  });
  it('remove → DELETE /api/custom-fields/{id}', async () => {
    await api.customFields.remove('f1');
    expect(lastCall()).toMatchObject({ url: '/api/custom-fields/f1', method: 'DELETE' });
  });
});

describe('api.search', () => {
  it('global → GET /api/search?q=', async () => {
    await api.search.global('hello');
    expect(lastCall().url).toBe('/api/search?q=hello');
  });
  it('inProject → GET /api/projects/{id}/search?q=', async () => {
    await api.search.inProject('p1', 'hi');
    expect(lastCall().url).toBe('/api/projects/p1/search?q=hi');
  });
});

// ─── Additional resource coverage ──────────────────────────────────────────────


describe('api.comments', () => {
  it('list → GET /api/items/{id}/comments', async () => {
    await api.comments.list('i1');
    expect(lastCall()).toMatchObject({ url: '/api/items/i1/comments', method: 'GET' });
  });
  it('create → POST /api/items/{id}/comments', async () => {
    await api.comments.create('i1', { content: 'hi' });
    expect(lastCall()).toMatchObject({ url: '/api/items/i1/comments', method: 'POST' });
  });
});

describe('api.dependencies', () => {
  it('list → GET /api/items/{id}/dependencies', async () => {
    await api.dependencies.list('i1');
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/dependencies',
      method: 'GET',
    });
  });
  it('create → POST /api/items/{id}/dependencies', async () => {
    await api.dependencies.create('i1', {
      target_item_id: 'i2',
      dependency_type: 'blocks',
    });
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/dependencies',
      method: 'POST',
    });
  });
  it('remove → DELETE /api/items/{id}/dependencies/{depId}', async () => {
    await api.dependencies.remove('i1', 'd1');
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/dependencies/d1',
      method: 'DELETE',
    });
  });
});

describe('api.roles', () => {
  it('list → GET /api/projects/{id}/roles', async () => {
    await api.roles.list('p1');
    expect(lastCall()).toMatchObject({ url: '/api/projects/p1/roles', method: 'GET' });
  });
  it('create → POST /api/projects/{id}/roles', async () => {
    await api.roles.create('p1', { name: 'Dev' });
    expect(lastCall()).toMatchObject({ url: '/api/projects/p1/roles', method: 'POST' });
  });
  it('remove → DELETE /api/roles/{id}', async () => {
    await api.roles.remove('r1');
    expect(lastCall()).toMatchObject({ url: '/api/roles/r1', method: 'DELETE' });
  });
  it('assign → PUT /api/items/{id}/roles/{roleId}', async () => {
    await api.roles.assign('i1', 'r1');
    expect(lastCall()).toMatchObject({ url: '/api/items/i1/roles/r1', method: 'PUT' });
  });
  it('unassign → DELETE /api/items/{id}/roles/{roleId}', async () => {
    await api.roles.unassign('i1', 'r1');
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/roles/r1',
      method: 'DELETE',
    });
  });
});

describe('api.attachments', () => {
  it('list → GET /api/items/{id}/attachments', async () => {
    await api.attachments.list('i1');
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/attachments',
      method: 'GET',
    });
  });
  it('upload → POST multipart, no JSON content-type', async () => {
    const file = new File(['x'], 'a.txt', { type: 'text/plain' });
    await api.attachments.upload('i1', file);
    const [url, init] = fetchMock.mock.calls.at(-1)!;
    expect(url).toBe('/api/items/i1/attachments');
    expect((init as RequestInit).method).toBe('POST');
    expect((init as RequestInit).body).toBeInstanceOf(FormData);
    const headers = (init as RequestInit).headers as Headers;
    expect(headers.has('Content-Type')).toBe(false);
  });
  it('remove → DELETE /api/attachments/{id}', async () => {
    await api.attachments.remove('a1');
    expect(lastCall()).toMatchObject({ url: '/api/attachments/a1', method: 'DELETE' });
  });
  it('downloadUrl returns the absolute API path', () => {
    expect(api.attachments.downloadUrl('a1')).toBe('/api/attachments/a1');
  });
});

describe('api.customFields values', () => {
  it('listValues → GET /api/items/{id}/custom-fields', async () => {
    await api.customFields.listValues('i1');
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/custom-fields',
      method: 'GET',
    });
  });
  it('setValue → PUT /api/items/{id}/custom-fields/{fieldId} with raw value body', async () => {
    await api.customFields.setValue('i1', 'f1', 'hello');
    const c = lastCall();
    expect(c).toMatchObject({
      url: '/api/items/i1/custom-fields/f1',
      method: 'PUT',
    });
    expect(JSON.parse(c.body as string)).toBe('hello');
  });
  it('clearValue → DELETE /api/items/{id}/custom-fields/{fieldId}', async () => {
    await api.customFields.clearValue('i1', 'f1');
    expect(lastCall()).toMatchObject({
      url: '/api/items/i1/custom-fields/f1',
      method: 'DELETE',
    });
  });
});

describe('api.data (export/import/backup/restore)', () => {
  it('exportProject → GET /api/projects/{id}/export?format=json', async () => {
    await api.data.exportProject('p1');
    expect(lastCall().url).toBe('/api/projects/p1/export?format=json');
  });
  it('exportProject csv → ...?format=csv', async () => {
    await api.data.exportProject('p1', 'csv');
    expect(lastCall().url).toBe('/api/projects/p1/export?format=csv');
  });
  it('importProject → POST /api/projects/import', async () => {
    await api.data.importProject({ project: {}, items: [] });
    expect(lastCall()).toMatchObject({ url: '/api/projects/import', method: 'POST' });
  });
  it('backup → GET /api/backup', async () => {
    await api.data.backup();
    expect(lastCall()).toMatchObject({ url: '/api/backup', method: 'GET' });
  });
  it('restore → POST /api/restore with raw bytes body (not multipart)', async () => {
    const blob = new Blob([new Uint8Array([1, 2, 3])]);
    await api.data.restore(blob);
    const c = lastCall();
    expect(c).toMatchObject({ url: '/api/restore', method: 'POST' });
    expect(c.body).toBe(blob);
  });
  it('importCsv → POST /projects/{id}/import-csv with text/csv body', async () => {
    await api.data.importCsv('p1', 'title\nFoo');
    const [url, init] = fetchMock.mock.calls.at(-1)!;
    expect(String(url)).toBe('/api/projects/p1/import-csv');
    expect(init?.method).toBe('POST');
    expect(init?.body).toBe('title\nFoo');
    expect((init?.headers as Headers).get('Content-Type')).toBe('text/csv');
  });
});
