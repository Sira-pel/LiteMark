export const LITEMARK_LOGO_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#2D2034"/>
      <stop offset="45%" stop-color="#1E1E24"/>
      <stop offset="100%" stop-color="#141416"/>
    </linearGradient>
    <linearGradient id="accentGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#FF6B35"/>
      <stop offset="50%" stop-color="#E95420"/>
      <stop offset="100%" stop-color="#C71585"/>
    </linearGradient>
    <linearGradient id="featherGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#FFFFFF"/>
      <stop offset="100%" stop-color="#DFDBD2"/>
    </linearGradient>
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx="0" dy="14" stdDeviation="16" flood-color="#000000" flood-opacity="0.55"/>
    </filter>
  </defs>
  <rect x="28" y="28" width="456" height="456" rx="112" fill="url(#bgGrad)"/>
  <rect x="29" y="29" width="454" height="454" rx="111" fill="none" stroke="rgba(255,255,255,0.12)" stroke-width="2"/>
  <circle cx="256" cy="256" r="160" fill="url(#accentGrad)" opacity="0.16"/>
  <g filter="url(#shadow)">
    <rect x="116" y="96" width="280" height="320" rx="24" fill="#242426" stroke="#38383C" stroke-width="3"/>
    <rect x="116" y="96" width="280" height="12" rx="6" fill="url(#accentGrad)"/>
    <line x1="160" y1="164" x2="352" y2="164" stroke="#44444A" stroke-width="6" stroke-linecap="round"/>
    <line x1="160" y1="204" x2="280" y2="204" stroke="#44444A" stroke-width="6" stroke-linecap="round"/>
    <path d="M 164 252 L 164 360 L 228 360" fill="none" stroke="url(#featherGrad)" stroke-width="18" stroke-linecap="round" stroke-linejoin="round"/>
    <path d="M 248 360 L 272 268 L 308 316 L 344 268 L 368 360" fill="none" stroke="url(#accentGrad)" stroke-width="18" stroke-linecap="round" stroke-linejoin="round"/>
    <polygon points="308,368 296,348 320,348" fill="url(#accentGrad)"/>
  </g>
</svg>`;

export const LiteMarkLogo: React.FC<{ className?: string; size?: number }> = ({ className = "w-5 h-5", size }) => {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 512 512"
      className={className}
      style={size ? { width: size, height: size } : undefined}
      fill="none"
      aria-label="LiteMark Logo"
    >
      <defs>
        <linearGradient id="logoBg" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#2D2034"/>
          <stop offset="45%" stopColor="#1E1E24"/>
          <stop offset="100%" stopColor="#141416"/>
        </linearGradient>
        <linearGradient id="logoAccent" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#FF6B35"/>
          <stop offset="50%" stopColor="#E95420"/>
          <stop offset="100%" stopColor="#C71585"/>
        </linearGradient>
        <linearGradient id="logoWhite" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#FFFFFF"/>
          <stop offset="100%" stopColor="#DFDBD2"/>
        </linearGradient>
      </defs>
      <rect x="28" y="28" width="456" height="456" rx="112" fill="url(#logoBg)"/>
      <rect x="29" y="29" width="454" height="454" rx="111" fill="none" stroke="rgba(255,255,255,0.15)" strokeWidth="3"/>
      <rect x="116" y="96" width="280" height="320" rx="24" fill="#242426" stroke="#38383C" strokeWidth="4"/>
      <rect x="116" y="96" width="280" height="14" rx="7" fill="url(#logoAccent)"/>
      <line x1="160" y1="164" x2="352" y2="164" stroke="#44444A" strokeWidth="8" strokeLinecap="round"/>
      <line x1="160" y1="204" x2="280" y2="204" stroke="#44444A" strokeWidth="8" strokeLinecap="round"/>
      <path d="M 164 252 L 164 360 L 228 360" fill="none" stroke="url(#logoWhite)" strokeWidth="20" strokeLinecap="round" strokeLinejoin="round"/>
      <path d="M 248 360 L 272 268 L 308 316 L 344 268 L 368 360" fill="none" stroke="url(#logoAccent)" strokeWidth="20" strokeLinecap="round" strokeLinejoin="round"/>
      <polygon points="308,368 296,348 320,348" fill="url(#logoAccent)"/>
    </svg>
  );
};
