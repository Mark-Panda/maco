import type { SVGProps } from "react";

export type MacoIconName =
  | "sessions"
  | "tasks"
  | "memory"
  | "skills"
  | "usage"
  | "jobs"
  | "settings"
  | "paperclip"
  | "x"
  | "panel-left"
  | "panel-right"
  | "sun"
  | "moon";

type MacoIconProps = {
  name: MacoIconName;
  size?: number;
  className?: string;
};

function baseProps(size: number, className?: string): SVGProps<SVGSVGElement> {
  return {
    xmlns: "http://www.w3.org/2000/svg",
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.65,
    strokeLinecap: "round",
    strokeLinejoin: "round",
    className: className ? `maco-icon ${className}` : "maco-icon",
    "aria-hidden": true,
  };
}

export function MacoIcon({ name, size = 20, className }: MacoIconProps) {
  const p = baseProps(size, className);

  switch (name) {
    case "sessions":
      return (
        <svg {...p}>
          <path d="M8 9h8" />
          <path d="M8 13h5" />
          <path d="M5 4h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H9l-4 3V6a2 2 0 0 1 2-2z" />
        </svg>
      );
    case "tasks":
      return (
        <svg {...p}>
          <path d="M9 6h11" />
          <path d="M9 12h11" />
          <path d="M9 18h7" />
          <path d="M5 6l1 1 2-2" />
          <path d="M5 12l1 1 2-2" />
          <path d="M5 18l1 1 2-2" />
        </svg>
      );
    case "memory":
      return (
        <svg {...p}>
          <path d="M12 3l1.2 3.6L17 7.8l-3.6 1.2L12 12.6 10.6 9 7 7.8l3.6-1.2L12 3z" />
          <path d="M5 14l.8 2.4L8.2 17l-2.4.8L5 20.2 4.2 17.8 1.8 17l2.4-.8L5 14z" />
          <path d="M19 14l.8 2.4L22.2 17l-2.4.8L19 20.2l-.8-2.4L15.8 17l2.4-.8L19 14z" />
        </svg>
      );
    case "skills":
      return (
        <svg {...p}>
          <path d="M12 2l2.4 7.2L22 12l-7.6 2.8L12 22l-2.4-7.2L2 12l7.6-2.8L12 2z" />
          <circle cx="12" cy="12" r="2.25" />
        </svg>
      );
    case "usage":
      return (
        <svg {...p}>
          <path d="M4 19V5" />
          <path d="M4 19h16" />
          <path d="M8 15v-3" />
          <path d="M12 15V9" />
          <path d="M16 15v-5" />
        </svg>
      );
    case "jobs":
      return (
        <svg {...p}>
          <circle cx="12" cy="12" r="8" />
          <path d="M12 8v4l2.5 2.5" />
          <path d="M16 3.5l1.5 1.5" />
        </svg>
      );
    case "settings":
      return (
        <svg {...p}>
          <path d="M4 7h3" />
          <path d="M4 12h6" />
          <path d="M4 17h4" />
          <circle cx="14" cy="7" r="2" />
          <circle cx="18" cy="12" r="2" />
          <circle cx="12" cy="17" r="2" />
          <path d="M10 7h10" />
          <path d="M8 12h8" />
          <path d="M14 17h6" />
        </svg>
      );
    case "paperclip":
      return (
        <svg {...p}>
          <path d="M8.5 12.5L14 7a3.5 3.5 0 1 1 5 5l-6.5 6.5a5 5 0 1 1-7-7l7-7" />
        </svg>
      );
    case "x":
      return (
        <svg {...p}>
          <path d="M6 6l12 12" />
          <path d="M18 6L6 18" />
        </svg>
      );
    case "panel-left":
      return (
        <svg {...p}>
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="M9 4v16" />
          <path d="M13 12h4" />
          <path d="M15 10v4" />
        </svg>
      );
    case "panel-right":
      return (
        <svg {...p}>
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="M15 4v16" />
          <path d="M7 12h4" />
          <path d="M9 10v4" />
        </svg>
      );
    case "sun":
      return (
        <svg {...p}>
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2" />
          <path d="M12 20v2" />
          <path d="M4.93 4.93l1.41 1.41" />
          <path d="M17.66 17.66l1.41 1.41" />
          <path d="M2 12h2" />
          <path d="M20 12h2" />
          <path d="M4.93 19.07l1.41-1.41" />
          <path d="M17.66 6.34l1.41-1.41" />
        </svg>
      );
    case "moon":
      return (
        <svg {...p}>
          <path d="M20 14.5A7.5 7.5 0 0 1 9.5 4 6.5 6.5 0 1 0 20 14.5z" />
        </svg>
      );
    default:
      return null;
  }
}
