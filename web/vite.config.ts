/// <reference types="vitest/config" />
import { defineConfig } from "vite";

// Fixed (unhashed) output filenames: `scripts/build-frontend.sh` copies the build
// output straight into `crates/looqlog/assets/` for `include_bytes!` embedding
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
  test: {
    // Vitest stubs CSS imports to an empty string by default, `?raw` included.
    // `entry-table-styles.test.ts` reads `style.css` as text to assert that the
    // row height there matches `ROW_HEIGHT` in the scroller (design D3 —
    // the height stopped being an inline style, so the two now live in different
    // files and can drift silently). Nothing renders CSS in these tests; this only
    // makes the file's own text readable.
    css: true,
  },
});
