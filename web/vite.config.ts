import { defineConfig } from "vite";

// Fixed (unhashed) output filenames: `scripts/build-frontend.sh` copies the build
// output straight into `crates/looq/assets/` for `include_bytes!` embedding
// (ADR-0008), and CI's `frontend-artifact-staleness` job diffs the committed copy
// against a fresh rebuild byte-for-byte. Content hashing would make every rebuild
// "stale" even with no source change, so filenames are pinned instead — see task
// 1.3 and the design.md D2 risk this addresses.
export default defineConfig({
  build: {
    sourcemap: false,
    rollupOptions: {
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
  worker: {
    format: "es",
    rollupOptions: {
      output: {
        entryFileNames: "assets/[name].js",
      },
    },
  },
});
