import { type Component, type JSX } from 'solid-js';

// Inline SVG glyphs lifted from the Tack design (Tack.dc.html), as Solid
// components so they aren't copy-pasted across the shell/board/drawer/palette.
// All stroke-based icons inherit `currentColor`, so callers set color via the
// surrounding token-styled element.

export interface IconProps {
  size?: number;
  class?: string;
  style?: JSX.CSSProperties;
  'stroke-width'?: number;
}

type Glyph = Component<IconProps>;

function stroke(
  path: JSX.Element,
  defaults?: { width?: number },
): Glyph {
  return (props) => (
    <svg
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width={props['stroke-width'] ?? defaults?.width ?? 1.6}
      class={props.class}
      style={props.style}
      aria-hidden="true"
    >
      {path}
    </svg>
  );
}

export const IconSearch = stroke(
  <>
    <circle cx="7" cy="7" r="4.5" />
    <path d="M11 11l3 3" stroke-linecap="round" />
  </>,
);

export const IconPlus = stroke(
  <path d="M8 3v10M3 8h10" stroke-linecap="round" stroke-width="2" />,
  { width: 2 },
);

export const IconChevronDown = stroke(
  <path d="M4 6l4 4 4-4" stroke-linecap="round" stroke-linejoin="round" />,
  { width: 1.5 },
);

export const IconChevronRight = stroke(
  <path d="M6 3l4 5-4 5" stroke-linecap="round" stroke-linejoin="round" />,
  { width: 1.5 },
);

export const IconBoard = stroke(
  <>
    <rect x="2" y="2.5" width="3.2" height="11" rx="1" />
    <rect x="6.4" y="2.5" width="3.2" height="7.5" rx="1" />
    <rect x="10.8" y="2.5" width="3.2" height="9.5" rx="1" />
  </>,
);

export const IconList = stroke(
  <path d="M5 4h9M5 8h9M5 12h9M2 4h.01M2 8h.01M2 12h.01" stroke-linecap="round" />,
);

export const IconTable = stroke(
  <>
    <rect x="2.2" y="3" width="11.6" height="10" rx="1.5" />
    <path d="M2.2 6.4h11.6M2.2 9.7h11.6M6.2 6.4V13" stroke-linecap="round" />
  </>,
  { width: 1.5 },
);

export const IconCalendar = stroke(
  <>
    <rect x="2.2" y="3" width="11.6" height="11" rx="2" />
    <path d="M2.2 6.2h11.6M5.5 1.8v2.4M10.5 1.8v2.4" stroke-linecap="round" />
  </>,
  { width: 1.5 },
);

export const IconTimeline = stroke(
  <path d="M2 4h7M5 8h7M3 12h6" stroke-linecap="round" />,
  { width: 1.5 },
);

export const IconSprint = stroke(
  <path d="M3.5 14V2.5M3.5 3c2-1.2 4.5-1.2 6.5 0s4.5 1.2 6.5 0M3.5 9c2-1.2 4.5-1.2 6.5 0" stroke-linecap="round" stroke-linejoin="round" />,
  { width: 1.5 },
);

export const IconOverview = stroke(
  <path d="M2 13h12M4.5 13V8M8 13V4M11.5 13V9.5" stroke-linecap="round" stroke-linejoin="round" />,
  { width: 1.5 },
);

export const IconProjects = stroke(
  <>
    <path d="M2 5.5l6-3 6 3-6 3z" />
    <path d="M2 5.5v5l6 3 6-3v-5" stroke-linecap="round" />
  </>,
  { width: 1.5 },
);

export const IconTemplates = stroke(
  <>
    <rect x="2.2" y="2.2" width="11.6" height="11.6" rx="2" />
    <path d="M2.2 6h11.6M6 6v7.8" stroke-linecap="round" />
  </>,
  { width: 1.5 },
);

export const IconSettings = stroke(
  <path d="M8 1.6a1.4 1.4 0 0 0-1.4 1.4 1.4 1.4 0 0 1-2.05.85 1.4 1.4 0 1 0-1.4 2.42 1.4 1.4 0 0 1 0 2.42 1.4 1.4 0 1 0 1.4 2.42 1.4 1.4 0 0 1 2.05.85 1.4 1.4 0 1 0 2.8 0 1.4 1.4 0 0 1 2.05-.85 1.4 1.4 0 1 0 1.4-2.42 1.4 1.4 0 0 1 0-2.42 1.4 1.4 0 1 0-1.4-2.42 1.4 1.4 0 0 1-2.05-.85A1.4 1.4 0 0 0 8 1.6z" stroke-linejoin="round" />,
  { width: 1.2 },
);

export const IconSun = stroke(
  <>
    <circle cx="8" cy="8" r="3.2" />
    <path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1" stroke-linecap="round" />
  </>,
  { width: 1.5 },
);

export const IconMoon = stroke(
  <path d="M13.5 9.2A5.6 5.6 0 0 1 6.8 2.5 5.6 5.6 0 1 0 13.5 9.2z" stroke-linejoin="round" />,
  { width: 1.5 },
);

export const IconFilter = stroke(
  <path d="M2 4h12M4 8h8M6 12h4" stroke-linecap="round" />,
);

export const IconLink = stroke(
  <path d="M6.5 9.5l3-3M5.5 7l-1.2 1.2a2.4 2.4 0 0 0 3.4 3.4L9 10.3M10.5 9l1.2-1.2a2.4 2.4 0 0 0-3.4-3.4L7 5.7" stroke-linecap="round" />,
  { width: 1.5 },
);

export const IconComment = stroke(
  <path d="M3 4.5h10v6H8l-3 2.5V10.5H3z" stroke-linejoin="round" />,
  { width: 1.5 },
);

export const IconClose = stroke(
  <path d="M4 4l8 8M12 4l-8 8" stroke-linecap="round" />,
  { width: 1.7 },
);

export const IconFleet = stroke(
  <>
    <circle cx="4.3" cy="4.3" r="2.1" />
    <circle cx="11.7" cy="4.3" r="2.1" />
    <circle cx="8" cy="11.2" r="2.1" />
    <path d="M5.9 5.7L7 9.6M10.1 5.7L9 9.6" stroke-linecap="round" />
  </>,
  { width: 1.4 },
);

export const IconApprovals = stroke(
  <>
    <path d="M3 8.4l3.2 3.2L13 4.8" stroke-linecap="round" stroke-linejoin="round" />
  </>,
  { width: 1.7 },
);

/** Bar-chart glyph for the unit-economics dashboard (card D5). */
export const IconEconomics = stroke(
  <>
    <path d="M2.5 13.5h11" stroke-linecap="round" />
    <path d="M4.5 13.5V8.8" stroke-linecap="round" />
    <path d="M8 13.5V4.5" stroke-linecap="round" />
    <path d="M11.5 13.5V7" stroke-linecap="round" />
  </>,
  { width: 1.5 },
);

export const IconAttachment = stroke(
  <path d="M11 5L6 10a2 2 0 0 0 2.8 2.8l5-5a3.4 3.4 0 0 0-4.8-4.8l-5 5a4.8 4.8 0 0 0 6.8 6.8L14 11" stroke-linecap="round" stroke-linejoin="round" />,
  { width: 1.5 },
);

/** Rocket glyph for the provisioning wizard (card D4) — "launch a new pod." */
export const IconProvision = stroke(
  <>
    <path
      d="M8 2.5c1.8 1 3 3.2 3 5.7 0 1.6-.4 3-1 4.1L8 14l-2-1.7c-.6-1.1-1-2.5-1-4.1 0-2.5 1.2-4.7 3-5.7z"
      stroke-linejoin="round"
    />
    <circle cx="8" cy="7.3" r="1.1" />
    <path d="M5.3 10.5L3.5 12M10.7 10.5L12.5 12" stroke-linecap="round" />
  </>,
  { width: 1.4 },
);

/** Agent glyph for the Agents nav entry — a small terminal/console shape,
 *  distinct from `IconFleet`'s three-node cluster. */
export const IconAgent = stroke(
  <>
    <rect x="2.2" y="3" width="11.6" height="10" rx="2" />
    <path d="M5 6.6L7 8.3L5 10" stroke-linecap="round" stroke-linejoin="round" />
    <path d="M8.4 10h2.6" stroke-linecap="round" />
  </>,
  { width: 1.4 },
);

/** Tack brand mark — teal pin with a check (uses accent + surface tokens). */
export const BrandMark: Component<{ size?: number; class?: string }> = (props) => (
  <svg
    width={props.size ?? 26}
    height={props.size ?? 26}
    viewBox="0 0 64 64"
    class={props.class}
    style={{ 'flex-shrink': 0 }}
    aria-hidden="true"
  >
    <path
      d="M32 4c-12 0-21 9-21 21 0 14 21 35 21 35s21-21 21-35C53 13 44 4 32 4z"
      fill="var(--color-primary-600)"
    />
    <circle cx="32" cy="25" r="13" fill="var(--color-bg-base)" />
    <path
      d="M25.5 25.5 L30 30 L39 20.5"
      fill="none"
      stroke="var(--color-primary-600)"
      stroke-width="4.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
);
