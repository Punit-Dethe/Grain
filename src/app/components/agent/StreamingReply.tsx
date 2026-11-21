import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AgentMarkdown } from "../markdown/Markdown";

/** Reveal speed: a steady floor, and a backlog that drains in about this
 * long, so text flows evenly however it arrives and never lags far behind. */
const MIN_CHARS_PER_SECOND = 90;
const DRAIN_SECONDS = 0.14;
/** The typing bubble's height: the reply grows out of it. */
const START_HEIGHT = 29;

/** Never cut a surrogate pair in half. */
function safeCut(text: string, end: number): number {
  const code = text.charCodeAt(end - 1);
  return code >= 0xd800 && code <= 0xdbff ? end + 1 : end;
}

/**
 * EXPERIMENT: the reply while it streams. Text is revealed at an even pace
 * (not in network bursts), the bubble's height glides to its content, new
 * paragraphs fade in, and the chat follows the end unless the user scrolls
 * away. `capAtStart` keeps the turn's start in view (a first reply reads
 * from its request down), so the finished reply lands without a jump.
 */
export function StreamingReply({
  text,
  capAtStart,
}: {
  text: string;
  capAtStart: boolean;
}) {
  const [shown, setShown] = useState(0);
  const [height, setHeight] = useState(START_HEIGHT);
  const turnRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const textRef = useRef(text);
  const shownRef = useRef(0);
  const followRef = useRef(true);
  textRef.current = text;

  // A reset ("" or a different reply) starts the reveal over.
  if (shownRef.current > text.length) {
    shownRef.current = 0;
    setShown(0);
  }

  useEffect(() => {
    const reduced = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    let frame = 0;
    let last = performance.now();
    let carry = 0;
    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const target = textRef.current.length;
      const backlog = target - shownRef.current;
      if (backlog > 0) {
        carry += reduced
          ? backlog
          : Math.max(MIN_CHARS_PER_SECOND, backlog / DRAIN_SECONDS) * dt;
        const step = Math.floor(carry);
        if (step > 0) {
          carry -= step;
          const next = safeCut(
            textRef.current,
            Math.min(target, shownRef.current + step),
          );
          shownRef.current = Math.min(target, next);
          setShown(shownRef.current);
        }
      } else {
        carry = 0;
      }
      follow();
      frame = requestAnimationFrame(tick);
    };
    const follow = () => {
      const turn = turnRef.current;
      const scroll = turn?.closest<HTMLElement>(".agc-c-scroll");
      if (!turn || !scroll || !followRef.current) return;
      let goal = scroll.scrollHeight - scroll.clientHeight;
      if (capAtStart) {
        const mark = scroll.querySelector<HTMLElement>(".agc-c-mark");
        if (mark) goal = Math.min(goal, mark.offsetTop - scroll.offsetTop);
      }
      if (goal > scroll.scrollTop) scroll.scrollTop = goal;
    };
    frame = requestAnimationFrame(tick);
    // Scrolling up hands the view back to the user for this reply.
    const scroll = turnRef.current?.closest<HTMLElement>(".agc-c-scroll");
    const onWheel = (event: WheelEvent) => {
      if (event.deltaY < 0) followRef.current = false;
    };
    scroll?.addEventListener("wheel", onWheel, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      scroll?.removeEventListener("wheel", onWheel);
    };
  }, [capAtStart]);

  // The bubble's height follows its content through a CSS transition.
  useLayoutEffect(() => {
    const inner = innerRef.current;
    if (!inner) return;
    const observer = new ResizeObserver(() => setHeight(inner.offsetHeight));
    observer.observe(inner);
    return () => observer.disconnect();
  }, []);

  const visible = text.slice(0, shown);
  return (
    <div ref={turnRef} className="agc-c-turn is-assistant is-streaming">
      <div className="agc-c-stream-box" style={{ height }}>
        <div ref={innerRef} className="agc-c-answer">
          <AgentMarkdown markdown={visible} />
        </div>
      </div>
      {/* The finished reply's tool row takes this room: no jump when it lands. */}
      <div className="agc-c-tools is-placeholder" aria-hidden="true" />
    </div>
  );
}
