import { splitProps, type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export type ButtonVariant =
  | 'primary'
  | 'secondary'
  | 'ghost'
  | 'danger'
  | 'success';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps
  extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

const SIZE: Record<ButtonSize, string> = {
  sm: 'px-3 py-1.5 text-sm',
  md: 'px-4 py-2 text-sm',
  lg: 'px-5 py-2.5 text-base',
};

function variantStyle(variant: ButtonVariant): JSX.CSSProperties {
  switch (variant) {
    case 'secondary':
      return {
        'background-color': 'var(--color-bg-base)',
        color: 'var(--color-text-primary)',
        border: '1px solid var(--color-border-medium)',
      };
    case 'ghost':
      return {
        'background-color': 'transparent',
        color: 'var(--color-text-secondary)',
      };
    case 'danger':
      return {
        'background-color': 'var(--color-danger-600)',
        color: 'var(--color-text-inverse)',
      };
    case 'success':
      return {
        'background-color': 'var(--color-success-600)',
        color: 'var(--color-text-inverse)',
      };
    case 'primary':
    default:
      // on-accent (not text-inverse): palette-aware, so a future palette
      // pairing a light accent with dark text still gets readable text
      // instead of a single hardcoded white.
      return {
        'background-color': 'var(--color-primary-600)',
        color: 'var(--color-on-accent)',
      };
  }
}

/** Token-driven button. Colors come only from CSS variables. */
const Button: Component<ButtonProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'variant',
    'size',
    'loading',
    'class',
    'style',
    'disabled',
    'children',
  ]);

  const isDisabled = () => local.disabled || local.loading;

  return (
    <button
      {...rest}
      disabled={isDisabled()}
      aria-busy={local.loading ? 'true' : undefined}
      class={clsx(
        'inline-flex items-center justify-center gap-2 rounded-lg font-medium transition-colors',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        SIZE[local.size ?? 'md'],
        local.class
      )}
      style={{
        ...variantStyle(local.variant ?? 'primary'),
        '--tw-ring-color': 'var(--color-focus-ring)',
        ...(typeof local.style === 'object' ? local.style : {}),
      }}
    >
      {local.loading && (
        <span
          class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-r-transparent"
          aria-hidden="true"
        />
      )}
      {local.children}
    </button>
  );
};

export default Button;
