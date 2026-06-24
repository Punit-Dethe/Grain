import React from "react";
import { inkOnOuter } from "./widgets";

interface ModuleFrameProps {
  /** Eyebrow label above the title, e.g. "MODULE A". */
  eyebrow: string;
  /** Big display title, e.g. "Configuration". */
  title: string;
  footerLeft: string;
  footerRight: string;
  children?: React.ReactNode;
}

/**
 * The module "rack card", ported 1:1 from ModuleA/B/C.qml:
 *  outer ColumnLayout margins 9 / top 15 / bottom 9
 *  → header Item h=40 (eyebrow 9px mono ABOVE Syne 17px title, spacing 2), m-x 6
 *  → travertine well: fillHeight, top 9 / bottom 5 / x 3, radius 14, inner shade
 *  → footer Item h=18, m-x 6.
 */
export const ModuleFrame: React.FC<ModuleFrameProps> = ({
  eyebrow,
  title,
  footerLeft,
  footerRight,
  children,
}) => {
  return (
    <div
      className="flex-1 min-w-0 flex flex-col"
      style={{
        borderRadius: 22,
        backgroundColor: "var(--qp-card-outer)",
        border: "1px solid var(--qp-card-outer-border)",
        paddingTop: 15,
        paddingLeft: 9,
        paddingRight: 9,
        paddingBottom: 9,
      }}
    >
      {/* Header — eyebrow stacked above the display title */}
      <div
        className="flex flex-col justify-center"
        style={{ height: 40, marginLeft: 6, marginRight: 6, gap: 2 }}
      >
        <span
          style={{
            fontFamily: "var(--qp-font-mono)",
            fontSize: 9,
            fontWeight: 700,
            letterSpacing: "2px",
            textTransform: "uppercase",
            color: inkOnOuter(0.4),
          }}
        >
          {eyebrow}
        </span>
        <span
          style={{
            fontFamily: "var(--qp-font-display)",
            fontSize: 17,
            fontWeight: 800,
            letterSpacing: "-0.3px",
            textTransform: "uppercase",
            lineHeight: 1,
            color: "var(--qp-card-outer-title)",
          }}
        >
          {title}
        </span>
      </div>

      {/* Travertine well */}
      <div
        className="relative flex-1 min-h-0 overflow-hidden"
        style={{
          marginTop: 9,
          marginBottom: 5,
          marginLeft: 3,
          marginRight: 3,
          borderRadius: 14,
          backgroundColor: "var(--qp-card-inner)",
          border: "1px solid var(--qp-card-inner-border)",
        }}
      >
        {/* simulated inner top shadow */}
        <div
          className="absolute left-0 right-0 top-0 pointer-events-none"
          style={{
            height: 10,
            background:
              "linear-gradient(var(--qp-card-inner-top-shade), transparent)",
          }}
        />
        <div
          className="relative w-full h-full flex flex-col"
          style={{ padding: "14px 14px 0 14px" }}
        >
          {children}
        </div>
      </div>

      {/* Footer */}
      <div
        className="flex items-center justify-between"
        style={{ height: 18, marginLeft: 6, marginRight: 6 }}
      >
        <span
          style={{
            fontFamily: "var(--qp-font-mono)",
            fontSize: 8,
            color: inkOnOuter(0.4),
          }}
        >
          {footerLeft}
        </span>
        <span
          style={{
            fontFamily: "var(--qp-font-mono)",
            fontSize: 8,
            color: inkOnOuter(0.4),
          }}
        >
          {footerRight}
        </span>
      </div>
    </div>
  );
};
