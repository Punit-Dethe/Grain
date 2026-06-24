/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        // Legacy name bridge → the GRAIN palette (see App.css @theme).
        text: "var(--color-text)",
        background: "var(--color-background)",
        "logo-primary": "var(--color-logo-primary)",
        "logo-stroke": "var(--color-logo-stroke)",
        "text-stroke": "var(--color-text-stroke)",
        // Status palette — warm, desaturated; used by status dots, alerts, badges.
        status: {
          ready: "var(--color-status-ready)",
          load: "var(--color-status-load)",
          warn: "var(--color-status-warn)",
          error: "var(--color-status-error)",
          idle: "var(--color-status-idle)",
          info: "var(--color-status-info)",
        },
      },
    },
  },
  plugins: [],
};
