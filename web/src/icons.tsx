type IconProps = { className?: string };

export function PlayIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M8 5v14l11-7z" />
    </svg>
  );
}

export function PauseIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M6 5h4v14H6zM14 5h4v14h-4z" />
    </svg>
  );
}

export function NextIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M6 5v14l9-7zM16 5h3v14h-3z" />
    </svg>
  );
}

export function PrevIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M18 5v14l-9-7zM5 5h3v14H5z" />
    </svg>
  );
}

export function HeartIcon({ className = "", filled = false }: IconProps & { filled?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      className={className}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.6l-1-1a5.5 5.5 0 0 0-7.8 7.8l1 1L12 21l7.8-7.6 1-1a5.5 5.5 0 0 0 0-7.8Z"
      />
    </svg>
  );
}

export function BanIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <circle cx="12" cy="12" r="8" strokeWidth="2" />
      <path strokeLinecap="round" strokeWidth="2" d="m7 17 10-10" />
    </svg>
  );
}

export function SearchIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <circle cx="11" cy="11" r="7" strokeWidth="2" />
      <path strokeLinecap="round" strokeWidth="2" d="m20 20-3.2-3.2" />
    </svg>
  );
}

export function LibraryIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <path strokeLinecap="round" strokeWidth="2" d="M5 5v14M10 5v14M15 6l4 13" />
    </svg>
  );
}

export function WaveIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <path
        strokeLinecap="round"
        strokeWidth="2"
        d="M3 12c2 0 2-5 4-5s2 10 4 10 2-10 4-10 2 5 4 5"
      />
    </svg>
  );
}

export function PlaylistIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <path strokeLinecap="round" strokeWidth="2" d="M4 7h11M4 12h11M4 17h7" />
      <circle cx="18" cy="16" r="2.5" strokeWidth="2" />
    </svg>
  );
}

export function GothicCrossIcon({ className = "" }: IconProps) {
  // Elongated Latin cross with barbed spear tips — wrought-iron gothic, not
  // the flared pattée (which reads Maltese/Armenian).
  const tip =
    "M0 -8 L1.7 -3.4 L4.8 -5.6 L2.1 -0.9 L1.5 1.2 L-1.5 1.2 L-2.1 -0.9 L-4.8 -5.6 L-1.7 -3.4 Z";
  return (
    <svg viewBox="0 0 32 44" fill="currentColor" className={className}>
      <g transform="translate(16 10)">
        <path d={tip} />
      </g>
      <g transform="translate(16 34) rotate(180)">
        <path d={tip} />
      </g>
      <g transform="translate(9 16) rotate(-90)">
        <path d={tip} />
      </g>
      <g transform="translate(23 16) rotate(90)">
        <path d={tip} />
      </g>
      <rect x="14.5" y="9" width="3" height="26" />
      <rect x="8" y="14.5" width="16" height="3" />
      <rect x="13" y="13" width="6" height="6" transform="rotate(45 16 16)" />
    </svg>
  );
}

export function QueueIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <path strokeLinecap="round" strokeWidth="2" d="M4 6h16M4 11h16M4 16h8" />
      <path
        fill="currentColor"
        stroke="none"
        d="M15.5 14.2a.6.6 0 0 1 .9-.52l4.2 2.42a.6.6 0 0 1 0 1.04l-4.2 2.42a.6.6 0 0 1-.9-.52z"
      />
    </svg>
  );
}

export function MusicNoteIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M12 3v10.55A4 4 0 1 0 14 17V7h4V3h-6z" />
    </svg>
  );
}

export function LyricsIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <path strokeLinecap="round" strokeWidth="2" d="M6 6h12M6 11h9M6 16h6" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="m15 16 2 2 4-5" />
    </svg>
  );
}

export function ChevronLeftIcon({ className = "" }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className={className}>
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="m15 6-6 6 6 6" />
    </svg>
  );
}
