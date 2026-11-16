// Stands in for `@openuidev/devtools` (aliased in vite.config.ts). OpenUI's
// react-lang auto-mounts that inspector widget into every page in dev builds;
// in the Agent notch it showed up as a floating panel. Rendering nothing here
// keeps it out for good, whatever the module load order.
export function OpenUIDevtools(): null {
  return null;
}
