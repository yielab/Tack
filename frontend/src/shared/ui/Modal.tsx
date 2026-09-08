import {
  type Component,
  type JSX,
  Show,
  createEffect,
  onCleanup,
} from 'solid-js';
import { Portal } from 'solid-js/web';
import clsx from 'clsx';

export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: JSX.Element;
  size?: 'sm' | 'md' | 'lg' | 'xl';
}

const SIZE: Record<NonNullable<ModalProps['size']>, string> = {
  sm: 'max-w-md',
  md: 'max-w-lg',
  lg: 'max-w-2xl',
  xl: 'max-w-4xl',
};

const Modal: Component<ModalProps> = (props) => {
  let dialog: HTMLDivElement | undefined;
  let previouslyFocused: HTMLElement | null = null;

  createEffect(() => {
    if (!props.isOpen) return;
    previouslyFocused = document.activeElement as HTMLElement | null;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        props.onClose();
      }
    };
    document.addEventListener('keydown', onKey);
    // Move focus into the dialog once mounted.
    queueMicrotask(() => dialog?.focus());

    onCleanup(() => {
      document.removeEventListener('keydown', onKey);
      previouslyFocused?.focus?.();
    });
  });

  // Solid's `Portal` re-propagates delegated events through the logical
  // owner tree, not the detached DOM subtree it actually mounts into — a
  // click anywhere in this dialog (Run, Cancel, a radio, a bare `<select>`)
  // still reaches whatever `onClick` sits on the component that opened it
  // (a board card's own row-click, say) unless stopped here, once, for the
  // whole modal.
  const handleBackdrop = (e: MouseEvent) => {
    e.stopPropagation();
    if (e.target === e.currentTarget) props.onClose();
  };

  return (
    <Show when={props.isOpen}>
      <Portal>
        <div
          class="fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-sm"
          style={{ 'background-color': 'var(--color-bg-overlay)' }}
          onClick={handleBackdrop}
        >
          <div
            ref={dialog}
            role="dialog"
            aria-modal="true"
            aria-label={props.title}
            tabindex={-1}
            class={clsx(
              'flex max-h-[90vh] w-full flex-col rounded-lg shadow-xl focus:outline-none',
              SIZE[props.size ?? 'md']
            )}
            style={{ 'background-color': 'var(--color-bg-elevated)' }}
          >
            <div
              class="flex items-center justify-between border-b p-6"
              style={{ 'border-color': 'var(--color-border-light)' }}
            >
              <h2
                class="text-xl font-semibold"
                style={{ color: 'var(--color-text-primary)' }}
              >
                {props.title}
              </h2>
              <button
                onClick={props.onClose}
                aria-label="Close"
                class="rounded p-1 transition-colors focus:outline-none focus-visible:ring-2"
                style={{
                  color: 'var(--color-text-tertiary)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                <svg class="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </button>
            </div>
            <div class="flex-1 overflow-y-auto p-6">{props.children}</div>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

export default Modal;
