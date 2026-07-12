import React from "react";

/** [GRAIN] A small mono count badge for Extensions group headers — tells the
 * user how many snippets / actions / modes they have at a glance. */
export const CountChip: React.FC<{ n: number }> = ({ n }) => (
  <span className="font-mono text-[0.6rem] font-semibold text-ink-soft tabular-nums bg-paper-sunken border border-line rounded-full min-w-[1.35rem] h-[1.15rem] px-1.5 flex items-center justify-center leading-none">
    {n}
  </span>
);
