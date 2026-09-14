"use client";

import { useEffect, useState } from "react";
import { TextMorph } from "torph/react";

const words = ["identity", "tenancy", "agent access"] as const;

export function HeroMorph() {
  const [index, setIndex] = useState(0);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return;
    }
    const timers = [window.setTimeout(() => setIndex(1), 800), window.setTimeout(() => setIndex(2), 2400)];
    return () => timers.forEach(window.clearTimeout);
  }, []);

  return (
    <span className="morph-slot">
      <TextMorph as="span" duration={180} ease="cubic-bezier(0.22, 1, 0.36, 1)" respectReducedMotion>
        {words[index]}
      </TextMorph>
    </span>
  );
}
