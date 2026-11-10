import { memo } from "react";

const PARTICLE_COUNT = 420;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));
const shell = ["", "", ""];

// Project an even sphere into an annulus, leaving the control label clear.
// Three paths let the layers move independently without one node per dot.
for (let index = 0; index < PARTICLE_COUNT; index++) {
  const depth = 1 - (2 * (index + 0.5)) / PARTICLE_COUNT;
  const radius = 22 + 25 * Math.sqrt(1 - depth * depth);
  const angle = index * GOLDEN_ANGLE;
  const x = (50 + radius * Math.cos(angle)).toFixed(1);
  const y = (50 + radius * Math.sin(angle)).toFixed(1);
  shell[index % shell.length] += `M${x} ${y}h.01`;
}

export const CrystalParticles = memo(function CrystalParticles({
  moving,
}: {
  moving: boolean;
}) {
  return (
    <svg
      className="meet-orb-particles"
      data-moving={moving}
      viewBox="0 0 100 100"
      aria-hidden="true"
    >
      <path d={shell[0]} />
      <path d={shell[1]} />
      <path d={shell[2]} />
    </svg>
  );
});
