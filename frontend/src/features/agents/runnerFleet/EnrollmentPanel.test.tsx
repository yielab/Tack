import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import EnrollmentPanel from './EnrollmentPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <EnrollmentPanel />, container);
  return { container, dispose };
}

// `Modal` renders through a `Portal` straight into `document.body` (a
// sibling of the mounted `container`, not a descendant) — so every helper
// here searches the whole document, and content assertions check
// `document.body.textContent` rather than `container.textContent`, to cover
// both the panel itself and any open modal.
function fillField(labelText: string, value: string) {
  const label = Array.from(document.querySelectorAll('label')).find((l) => l.textContent?.startsWith(labelText))!;
  const input = document.getElementById(label.getAttribute('for')!) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

function clickButton(text: string) {
  const btn = Array.from(document.querySelectorAll('button')).find((b) => b.textContent?.includes(text))!;
  btn.click();
}

const ENROLL_RESULT = {
  protocol_version: 1,
  runner_id: 'runr_1',
  token_id: 'ent_1',
  enrollment_token: 'enr_super-secret-value',
  expires_at: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
};

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('EnrollmentPanel — states the GET /runners gap up front', () => {
  it('names the missing list endpoint rather than implying a full roster', () => {
    mount();
    expect(document.body.textContent).toMatch(/no endpoint yet to list existing runners/i);
  });
});

describe('EnrollmentPanel — enrollment and the one-time token', () => {
  it('enrolls a runner and shows the token exactly once, with an explicit "shown once" warning', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runners/enrollment') && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify(ENROLL_RESULT), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    mount();
    fillField('Name', 'runner-a');
    await flush();
    clickButton('Enroll');
    await flush();

    expect(document.body.textContent).toContain('enr_super-secret-value');
    expect(document.body.textContent).toContain('Shown once');
    expect(document.body.textContent).toContain('will never be shown again');

    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/runners/enrollment') && (c[1] as RequestInit)?.method === 'POST',
    );
    expect(postCall).toBeTruthy();
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toMatchObject({ name: 'runner-a', total_capacity: 1, available_capacity: 1 });
  });

  it('never shows the token again once the modal is closed — the credential-displays-once acceptance bar', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runners/enrollment') && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify(ENROLL_RESULT), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    mount();
    fillField('Name', 'runner-a');
    await flush();
    clickButton('Enroll');
    await flush();
    expect(document.body.textContent).toContain('enr_super-secret-value');

    clickButton("I've copied it — close");
    await flush();

    // The dialog itself unmounts (Modal's Show), and the raw secret string
    // is gone from the DOM entirely — not merely visually hidden.
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.body.textContent).not.toContain('enr_super-secret-value');
  });

  it('adds the enrolled runner to the session list as "Connection unconfirmed", never "Healthy"', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runners/enrollment') && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify(ENROLL_RESULT), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    mount();
    fillField('Name', 'runner-a');
    await flush();
    clickButton('Enroll');
    await flush();
    clickButton("I've copied it — close");
    await flush();

    expect(document.body.textContent).toContain('runner-a');
    expect(document.body.textContent).toContain('Connection unconfirmed');
    expect(document.body.textContent).not.toContain('Healthy');
  });

  it('rejects malformed JSON labels before ever calling the API', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 200 }));
    mount();
    fillField('Name', 'runner-a');
    fillField('Labels', '{not valid json');
    await flush();
    clickButton('Enroll');
    await flush();

    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/api/runners/enrollment'))).toBe(false);
  });
});

describe('EnrollmentPanel — clipboard copy', () => {
  it('copies the token via the Clipboard API and reflects success in the button label', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runners/enrollment') && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify(ENROLL_RESULT), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

    mount();
    fillField('Name', 'runner-a');
    await flush();
    clickButton('Enroll');
    await flush();
    clickButton('Copy token');
    await flush();

    expect(writeText).toHaveBeenCalledWith('enr_super-secret-value');
    expect(document.body.textContent).toContain('Copied');
  });
});

describe('EnrollmentPanel — revocation', () => {
  it('revokes a session-enrolled runner and marks it Unconfigured, removing the revoke action', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runners/enrollment') && method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify(ENROLL_RESULT), { status: 200 }));
      }
      if (url.endsWith('/api/runners/runr_1/revoke') && method === 'POST') {
        return Promise.resolve(
          new Response(JSON.stringify({ protocol_version: 1, runner_id: 'runr_1', state: 'revoked' }), { status: 200 }),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    mount();
    fillField('Name', 'runner-a');
    await flush();
    clickButton('Enroll');
    await flush();
    clickButton("I've copied it — close");
    await flush();

    clickButton('Revoke runner');
    await flush();

    expect(document.body.textContent).toContain('Unconfigured');
    expect(
      Array.from(document.querySelectorAll('button')).some((b) => b.textContent?.includes('Revoke runner')),
    ).toBe(false);
    expect(
      fetchMock.mock.calls.some(
        (c) => String(c[0]).endsWith('/api/runners/runr_1/revoke') && (c[1] as RequestInit)?.method === 'POST',
      ),
    ).toBe(true);
  });

  it('revokes a runner by manually-entered ID, for one enrolled outside this session', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runners/runr_elsewhere/revoke') && method === 'POST') {
        return Promise.resolve(
          new Response(JSON.stringify({ protocol_version: 1, runner_id: 'runr_elsewhere', state: 'revoked' }), {
            status: 200,
          }),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    fillField('Runner ID', 'runr_elsewhere');
    await flush();
    const revokeForm = Array.from(container.querySelectorAll('form')).find((f) =>
      f.textContent?.includes('Revoke a runner by ID'),
    )!;
    const submitBtn = revokeForm.querySelector('button[type="submit"]') as HTMLButtonElement;
    submitBtn.click();
    await flush();

    expect(document.body.textContent).toContain('runr_elsewhere');
    expect(document.body.textContent).toContain('Unconfigured');
    expect(
      fetchMock.mock.calls.some(
        (c) =>
          String(c[0]).endsWith('/api/runners/runr_elsewhere/revoke') && (c[1] as RequestInit)?.method === 'POST',
      ),
    ).toBe(true);
  });
});
