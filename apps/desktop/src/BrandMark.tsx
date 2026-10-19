import { useId } from "react";

/**
 * The Nulis mark (a paper sheet, a batik ribbon, and a pen forming an N)
 * drawn in the current theme's colors. The application icon keeps its own
 * fixed colors (`src-tauri/icons/nulis-source.svg`); this in-window copy
 * follows the theme so it sits naturally in every color scheme. Tints are
 * layered with opacity rather than `color-mix`, which older webviews lack.
 */
export function BrandMark({ className }: { className?: string }) {
  // Several marks can be on screen, so definition IDs are per instance.
  const id = useId().replace(/:/g, "");
  const ribbon = `${id}-ribbon`;
  const rim = `${id}-rim`;
  const petal = `${id}-petal`;
  const flower = `${id}-flower`;
  return <svg className={className} viewBox="80 80 864 864" aria-hidden="true" focusable="false">
    <defs>
      <mask id={ribbon} maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024">
        <line x1="430" y1="352" x2="695" y2="745" stroke="#fff" strokeWidth="130" />
        <rect x="630" y="415" width="130" height="375" rx="65" fill="#fff" />
      </mask>
      <mask id={rim} maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024">
        <line x1="430" y1="352" x2="695" y2="745" stroke="#fff" strokeWidth="150" />
        <rect x="620" y="405" width="150" height="395" rx="75" fill="#fff" />
      </mask>
      <path id={petal} d="M0 -8C-15 -30 -14 -54 0 -70C14 -54 15 -30 0 -8Z" />
      <g id={flower}>
        {[0, 60, 120, 180, 240, 300].map((angle) => <use key={angle} href={`#${petal}`} transform={`rotate(${angle})`} />)}
        <circle r="16" className="mark-flower-center" />
      </g>
    </defs>
    <rect className="mark-tile" x="80" y="80" width="864" height="864" rx="196" />
    <rect className="mark-tile-edge" x="82" y="82" width="860" height="860" rx="194" />
    {/* Ribbon and pen barrel: a gold-like rim, then a tinted ground with batik flowers. */}
    <rect className="mark-rim" x="280" y="180" width="560" height="660" mask={`url(#${rim})`} />
    <g mask={`url(#${ribbon})`}>
      <rect className="mark-ribbon" x="300" y="200" width="520" height="620" />
      <rect className="mark-ribbon-tint" x="300" y="200" width="520" height="620" />
      <g className="mark-flowers">
        <use href={`#${flower}`} transform="translate(545 485) scale(1.15)" />
        <use href={`#${flower}`} transform="translate(700 590) scale(.85)" />
        <use href={`#${flower}`} transform="translate(680 755) scale(.6)" />
        <use href={`#${flower}`} transform="translate(712 455) scale(.5)" />
      </g>
    </g>
    {/* The paper sheet with its curl and folded corner. */}
    <path className="mark-paper"
      d="M250 790V340C250 302 280 272 318 272H440C474 272 498 290 516 318L548 366C512 368 466 392 428 434V790Z" />
    <path className="mark-shade" d="M428 434C466 392 512 368 548 366C520 356 488 356 456 366C444 384 434 406 428 434Z" />
    <path className="mark-shade" d="M250 790V646C298 690 356 708 428 694V790Z" />
    <g className="mark-lines">
      <rect x="292" y="392" width="94" height="20" rx="10" />
      <rect x="292" y="434" width="104" height="20" rx="10" />
      <rect x="292" y="476" width="78" height="20" rx="10" />
    </g>
    {/* The pen nib and band. */}
    <path className="mark-nib" d="M640 396V346C640 300 664 256 695 202C726 256 750 300 750 346V396Z" />
    <path className="mark-slit" d="M695 216V318" />
    <circle className="mark-hole" cx="695" cy="328" r="14" />
    <rect className="mark-nib" x="622" y="392" width="146" height="36" rx="10" />
  </svg>;
}
