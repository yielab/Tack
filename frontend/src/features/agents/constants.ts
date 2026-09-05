// Every console-step command string these steps introduce, in one place —
// so a docs page can cite this file instead of a string embedded in JSX.
// `ExecutionToggle.tsx` and `ProviderKeyPanel.tsx` still hold their own two
// command strings inline (`tack serve --with-runner`,
// `tack runner secret set {name}`) — this file does not re-export theirs,
// only the ones these steps add.

/** Per-harness vendor install command, shown when step 2's probe reports the
 *  binary absent from `PATH`. Verified against the vendor's own published
 *  npm package (`@anthropic-ai/claude-code`, `@openai/codex`) rather than
 *  guessed. Keyed by the wire `harness_kind` values
 *  `shared/runWithAgent/shared.ts#HARNESS_KINDS` already establishes. */
export const HARNESS_INSTALL_COMMAND: Readonly<Record<string, string>> = {
  'claude-code': 'npm install -g @anthropic-ai/claude-code',
  codex: 'npm install -g @openai/codex',
};

/** Per-harness vendor login command — step 3's "use the agent's own login"
 *  path. Each is the harness's own CLI entry point; Tack never touches
 *  vendor credentials (ADR 0050/0058), so this is the whole of what Tack can
 *  tell the operator to run. */
export const HARNESS_LOGIN_COMMAND: Readonly<Record<string, string>> = {
  'claude-code': 'claude',
  codex: 'codex login',
};

/** The one sentence every "use the agent's own login" row carries — Tack
 *  has no route that reads back whether a vendor login succeeded, so this
 *  is the honest state rather than a guess. A shared
 *  constant keeps every row's wording identical instead of near-duplicates. */
export const CANNOT_OBSERVE_VENDOR_LOGIN =
  'Tack cannot see whether this succeeded — confirm with a test run below.';
