// OpenUI's react-lang auto-mounts its devtools widget into the page in dev
// builds (a top-level side effect of importing it). The notch is a tiny
// transparent window, so set the library's own "already mounted" flag first.
// Must be imported before `@openuidev/react-lang` (module order is import order).
(globalThis as Record<symbol, unknown>)[
  Symbol.for("openui.devtools.autoMount")
] = true;

export {};
