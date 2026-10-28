// Bundle the actual TSX component with the existing Vite toolchain, then run
// Node's tests. No browser, development server, or generated WASM is required.
import { build } from "vite";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = fileURLToPath(new URL("../", import.meta.url));
const output = await mkdtemp(join(tmpdir(), "amend-document-tests-"));
try {
  await build({
    configFile: false,
    root,
    logLevel: "error",
    esbuild: { jsx: "automatic" },
    ssr: { noExternal: true },
    build: {
      ssr: join(root, "tests/amend-document.test.tsx"),
      outDir: output,
      emptyOutDir: true,
      rollupOptions: {
        external: [/^node:/],
        output: { entryFileNames: "amend-document.test.mjs" },
      },
    },
  });
  const result = spawnSync(process.execPath, ["--test", join(output, "amend-document.test.mjs")], { stdio: "inherit" });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
} finally {
  await rm(output, { recursive: true, force: true });
}
