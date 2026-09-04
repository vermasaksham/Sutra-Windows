/**
 * Inline SVG icons. Kept local rather than pulled from an icon package: the
 * set is small, and every one of them inherits `currentColor` so they theme
 * for free.
 */
import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement>;

function Svg({ children, ...props }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      {...props}
    >
      {children}
    </svg>
  );
}

function Heading({ level, ...props }: IconProps & { level: 1 | 2 | 3 }) {
  return (
    <Svg {...props}>
      <path d="M5 5v14M13 5v14M5 12h8" />
      <text
        x="16.5"
        y="19"
        fontSize="10"
        fontWeight="600"
        stroke="none"
        fill="currentColor"
      >
        {level}
      </text>
    </Svg>
  );
}

export const H1Icon = (p: IconProps) => <Heading level={1} {...p} />;
export const H2Icon = (p: IconProps) => <Heading level={2} {...p} />;
export const H3Icon = (p: IconProps) => <Heading level={3} {...p} />;

export const TextIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 6h16M4 12h16M4 18h10" />
  </Svg>
);

export const BulletListIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M9 6h11M9 12h11M9 18h11" />
    <circle cx="4.5" cy="6" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="4.5" cy="12" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="4.5" cy="18" r="1.1" fill="currentColor" stroke="none" />
  </Svg>
);

export const OrderedListIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M10 6h10M10 12h10M10 18h10" />
    <text x="2" y="8" fontSize="7.5" stroke="none" fill="currentColor">
      1
    </text>
    <text x="2" y="14.5" fontSize="7.5" stroke="none" fill="currentColor">
      2
    </text>
    <text x="2" y="21" fontSize="7.5" stroke="none" fill="currentColor">
      3
    </text>
  </Svg>
);

export const TaskListIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="4" width="7" height="7" rx="1.6" />
    <rect x="3" y="14" width="7" height="7" rx="1.6" />
    <path d="M4.8 7.4l1.3 1.3 2.2-2.4M13 7.5h8M13 17.5h8" />
  </Svg>
);

export const CodeIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8.5 8L4 12l4.5 4M15.5 8l4.5 4-4.5 4" />
  </Svg>
);

export const QuoteIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M5 5v14" />
    <path d="M10 8h9M10 12h9M10 16h5" />
  </Svg>
);

export const DividerIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 12h18" />
  </Svg>
);

export const TableIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="4.5" width="18" height="15" rx="2" />
    <path d="M3 9.5h18M9.5 9.5v10" />
  </Svg>
);

export const ImageIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="4.5" width="18" height="15" rx="2" />
    <circle cx="8.5" cy="10" r="1.6" />
    <path d="M4 17l4.5-4.5 3.5 3.5 3-2.5L20 17" />
  </Svg>
);

export const MathIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 19c2.5 0 3-1.5 3.6-4.2L9.6 6C10 4.3 10.8 3 12.5 3c1 0 1.7.4 2 .9" />
    <path d="M6.5 10.5h6" />
    <path d="M14 12l6 7M20 12l-6 7" />
  </Svg>
);

export const ChemistryIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M10 3v6.2L4.6 18A2 2 0 0 0 6.3 21h11.4a2 2 0 0 0 1.7-3L14 9.2V3" />
    <path d="M8.5 3h7" />
    <path d="M7.2 14.5h9.6" />
  </Svg>
);

export const GripIcon = (p: IconProps) => (
  <Svg {...p} strokeWidth={0}>
    {[7, 12, 17].map((cy) =>
      [9, 15].map((cx) => (
        <circle
          key={`${cx}-${cy}`}
          cx={cx}
          cy={cy}
          r="1.4"
          fill="currentColor"
        />
      )),
    )}
  </Svg>
);
