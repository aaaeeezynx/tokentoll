/**
 * SF Symbols 風格圖標（手繪 SVG，Windows 可用）。
 * 命名 1:1 對照 SF Symbols 語義；24x24，圓角端點，描邊 1.8。
 */
export type IconName =
  | "chart-bar"
  | "chart-bar-fill"
  | "cpu"
  | "calendar"
  | "calendar-day"
  | "clock"
  | "chart-line"
  | "switch" // arrow.left.arrow.right（來源/切換）
  | "server"
  | "key"
  | "key-fill"
  | "gear"
  | "gear-fill"
  | "plus"
  | "pencil"
  | "trash"
  | "copy"
  | "play-fill"
  | "stop-fill"
  | "download"
  | "check"
  | "x"
  | "search"
  | "chevron-right"
  | "chevron-left"
  | "chevron-down"
  | "bolt-fill"
  | "alert"
  | "sliders"
  | "info"
  | "calculator"
  | "eye"
  | "eye-slash"
  | "refresh"
  | "power";

function Paths({ name }: { name: IconName }) {
  switch (name) {
    case "chart-bar":
      return (
        <>
          <path d="M4 20h16" />
          <path d="M7 20v-6" />
          <path d="M12 20V6" />
          <path d="M17 20v-9" />
        </>
      );
    case "chart-bar-fill":
      return (
        <>
          <rect x="5.2" y="12.5" width="3.6" height="7.5" rx="1.2" fill="currentColor" stroke="none" />
          <rect x="10.2" y="4.5" width="3.6" height="15.5" rx="1.2" fill="currentColor" stroke="none" />
          <rect x="15.2" y="9.5" width="3.6" height="10.5" rx="1.2" fill="currentColor" stroke="none" />
        </>
      );
    case "cpu":
      return (
        <>
          <rect x="7" y="7" width="10" height="10" rx="2" />
          <rect x="10.4" y="10.4" width="3.2" height="3.2" rx="0.6" />
          <path d="M9.5 2.5v3M14.5 2.5v3M9.5 18.5v3M14.5 18.5v3M2.5 9.5h3M2.5 14.5h3M18.5 9.5h3M18.5 14.5h3" />
        </>
      );
    case "calendar":
      return (
        <>
          <rect x="4" y="5.5" width="16" height="15" rx="2.5" />
          <path d="M4 10.5h16" />
          <path d="M8.5 3.5v4M15.5 3.5v4" />
        </>
      );
    case "calendar-day":
      return (
        <>
          <rect x="4" y="5.5" width="16" height="15" rx="2.5" />
          <path d="M4 10.5h16" />
          <path d="M8.5 3.5v4M15.5 3.5v4" />
          <circle cx="12" cy="15" r="1.4" fill="currentColor" stroke="none" />
        </>
      );
    case "clock":
      return (
        <>
          <circle cx="12" cy="12" r="8" />
          <path d="M12 7.5V12l3 2" />
        </>
      );
    case "chart-line":
      return (
        <>
          <path d="M4 4v15.5h16" />
          <path d="M7.5 14.5l3.5-4 3 2.5 4.5-5.5" />
          <path d="M15.5 7.5h3v3" />
        </>
      );
    case "switch":
      return (
        <>
          <path d="M4 8.5h13" />
          <path d="M14 5.5l3 3-3 3" />
          <path d="M20 15.5H7" />
          <path d="M10 12.5l-3 3 3 3" />
        </>
      );
    case "server":
      return (
        <>
          <rect x="4" y="4" width="16" height="16" rx="2.5" />
          <path d="M4 9.5h16M4 14.5h16" />
          <circle cx="7" cy="6.7" r="1" fill="currentColor" stroke="none" />
          <circle cx="7" cy="12" r="1" fill="currentColor" stroke="none" />
          <circle cx="7" cy="17.3" r="1" fill="currentColor" stroke="none" />
          <path d="M14 6.7h3M14 12h3M14 17.3h3" />
        </>
      );
    case "key":
      return (
        <>
          <circle cx="8" cy="12" r="4" />
          <path d="M12 12h9" />
          <path d="M17.5 12v3.5M20.5 12v2.5" />
        </>
      );
    case "key-fill":
      return (
        <>
          <circle cx="7.6" cy="12" r="4.4" fill="currentColor" stroke="none" />
          <circle cx="7.6" cy="12" r="1.6" fill="none" stroke="#1a1a21" strokeWidth="1.6" />
          <path d="M11.8 12H21" strokeWidth="2.6" />
          <path d="M16.8 12v4M19.8 12v3" strokeWidth="2.2" />
        </>
      );
    case "gear":
    case "gear-fill":
      return (
        <>
          <circle cx="12" cy="12" r="5.6" strokeWidth={name === "gear-fill" ? 2.6 : 1.8} />
          <circle cx="12" cy="12" r="1.8" />
          <path
            d="M12 2.6v2.6M12 18.8v2.6M2.6 12h2.6M18.8 12h2.6M5.4 5.4l1.8 1.8M16.8 16.8l1.8 1.8M18.6 5.4l-1.8 1.8M7.2 16.8l-1.8 1.8"
            strokeWidth="2.2"
          />
        </>
      );
    case "plus":
      return (
        <>
          <path d="M12 5v14M5 12h14" />
        </>
      );
    case "pencil":
      return (
        <>
          <path d="M14.5 5.5l4 4L8 20H4v-4L14.5 5.5z" />
          <path d="M12.5 7.5l4 4" />
        </>
      );
    case "trash":
      return (
        <>
          <path d="M4.5 6.5h15M9.5 6V4.8a1 1 0 011-1h3a1 1 0 011 1V6" />
          <path d="M6.5 6.5l1 12.6a1.5 1.5 0 001.5 1.4h6a1.5 1.5 0 001.5-1.4l1-12.6" />
          <path d="M10 10.5v6M14 10.5v6" />
        </>
      );
    case "copy":
      return (
        <>
          <rect x="9" y="9" width="11" height="11" rx="2" />
          <path d="M5 15V6a2 2 0 012-2h9" />
        </>
      );
    case "play-fill":
      return (
        <path
          d="M8.5 5.8v12.4c0 .8.9 1.3 1.6.9l9.6-6.2c.6-.4.6-1.4 0-1.8L10.1 4.9c-.7-.4-1.6.1-1.6.9z"
          fill="currentColor"
          stroke="none"
        />
      );
    case "stop-fill":
      return <rect x="7" y="7" width="10" height="10" rx="2" fill="currentColor" stroke="none" />;
    case "download":
      return (
        <>
          <rect x="4" y="4" width="16" height="16" rx="3" />
          <path d="M12 8v7" />
          <path d="M9.2 12.5L12 15.3l2.8-2.8" />
        </>
      );
    case "check":
      return <path d="M5 12.5l4.5 4.5L19 7.5" />;
    case "x":
      return <path d="M6 6l12 12M18 6L6 18" />;
    case "search":
      return (
        <>
          <circle cx="11" cy="11" r="6.5" />
          <path d="M15.8 15.8L20 20" />
        </>
      );
    case "chevron-right":
      return <path d="M9.5 5.5L16 12l-6.5 6.5" />;
    case "chevron-left":
      return <path d="M14.5 5.5L8 12l6.5 6.5" />;
    case "chevron-down":
      return <path d="M5.5 9.5L12 16l6.5-6.5" />;
    case "bolt-fill":
      return (
        <path
          d="M13 2.5L4.5 13.5H11l-1 8 8.5-11H12l1-8z"
          fill="currentColor"
          stroke="none"
        />
      );
    case "alert":
      return (
        <>
          <path d="M12 4L21 19.5H3L12 4z" />
          <path d="M12 10v4" />
          <circle cx="12" cy="16.6" r="1" fill="currentColor" stroke="none" />
        </>
      );
    case "sliders":
      return (
        <>
          <path d="M4 7h16M4 12h16M4 17h16" />
          <circle cx="9" cy="7" r="2.1" fill="currentColor" stroke="none" />
          <circle cx="15" cy="12" r="2.1" fill="currentColor" stroke="none" />
          <circle cx="8" cy="17" r="2.1" fill="currentColor" stroke="none" />
        </>
      );
    case "info":
      return (
        <>
          <circle cx="12" cy="12" r="8.5" />
          <path d="M12 11v5" />
          <circle cx="12" cy="7.8" r="1.1" fill="currentColor" stroke="none" />
        </>
      );
    case "eye":
      return (
        <>
          <path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z" />
          <circle cx="12" cy="12" r="3" />
        </>
      );
    case "eye-slash":
      return (
        <>
          <path d="M4 4l16 16" />
          <path d="M10.6 5.9c.5-.1.9-.1 1.4-.1 6 0 9.5 6.2 9.5 6.2a17.2 17.2 0 01-2.7 3.3" />
          <path d="M6.6 6.6A16.6 16.6 0 002.5 12S6 18.5 12 18.5c1.2 0 2.3-.2 3.3-.6" />
        </>
      );
    case "refresh":
      return (
        <>
          <path d="M19 8.5A7.5 7.5 0 004.5 12" />
          <path d="M4.5 8.5V12H8" />
          <path d="M5 15.5A7.5 7.5 0 0019.5 12" />
          <path d="M19.5 15.5V12H16" />
        </>
      );
    case "power":
      return (
        <>
          <path d="M12 3.5v7" />
          <path d="M7 6.2a7.5 7.5 0 1010 0" />
        </>
      );
    case "calculator":
      return (
        <>
          <rect x="5.5" y="3.5" width="13" height="17" rx="2.5" />
          <path d="M8.5 7.5h7" />
          {[11, 14, 17].map((y) =>
            [8.2, 12, 15.8].map((x) => (
              <circle key={`${x}-${y}`} cx={x} cy={y} r="1.05" fill="currentColor" stroke="none" />
            )),
          )}
        </>
      );
  }
}

export function Icon({
  name,
  size = 16,
  className,
  strokeWidth = 1.8,
}: {
  name: IconName;
  size?: number;
  className?: string;
  strokeWidth?: number;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <Paths name={name} />
    </svg>
  );
}
