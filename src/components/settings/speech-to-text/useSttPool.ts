/**
 * [GRAIN] useSttPool — thin hook wrapper over the singleton sttPoolStore.
 *
 * The settings panel imports this hook; the quick panel uses the store
 * directly. Both share the same Zustand state, so any mutation in one
 * is immediately visible in the other without a second network round-trip.
 */
export { useSttPoolStore as useSttPool } from "@/stores/sttPoolStore";
export type { SttPoolStore as SttPoolState } from "@/stores/sttPoolStore";
