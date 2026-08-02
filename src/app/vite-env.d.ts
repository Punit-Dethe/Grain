/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_GRAIN_UI?: "next";
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
