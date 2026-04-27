import type { Plugin } from "vite";
import { NodeIO, Logger } from "@gltf-transform/core";
import { ALL_EXTENSIONS } from "@gltf-transform/extensions";
import { meshopt, weld, dedup, instance, palette, prune, simplify } from "@gltf-transform/functions";
import { MeshoptEncoder, MeshoptDecoder, MeshoptSimplifier } from "meshoptimizer";
import { readFileSync } from "node:fs";

const io = new NodeIO()
  .registerExtensions(ALL_EXTENSIONS)
  .registerDependencies({ 'meshopt.encoder': MeshoptEncoder })
  .setLogger(new Logger(Logger.Verbosity.ERROR));
const ready = Promise.all([MeshoptEncoder.ready, MeshoptDecoder.ready]);

async function compressGlb(source: Buffer): Promise<Buffer> {
  await ready;
  const doc = await io.readBinary(source);
  await doc.transform(dedup(),
    instance({ min: 5 }),
    palette({ min: 5 }),
    weld(),
    simplify({
      simplifier: MeshoptSimplifier,
      error: 0.001,
    }),
    prune(),
    meshopt({ encoder: MeshoptEncoder, level: "medium" })
  );
  return Buffer.from(await io.writeBinary(doc));
}

// Dev-server route prefix for serving compressed GLBs
const DEV_PREFIX = "/@glb";

export function glbCompressPlugin(): Plugin {
  // Keyed by absolute file path; shared across dev and build
  const cache = new Map<string, Promise<Buffer>>();
  let isDev = false;

  function getCompressed(filePath: string): Promise<Buffer> {
    if (!cache.has(filePath)) {
      cache.set(
        filePath,
        (async () => {
          const source = Buffer.from(readFileSync(filePath));
          const compressed = await compressGlb(source);
          const ratio = ((compressed.length / source.length) * 100).toFixed(1);
          console.log(
            `  glb-compress: ${filePath.split("/").pop()} ` +
            `${(source.length / 1024).toFixed(0)} KB → ` +
            `${(compressed.length / 1024).toFixed(0)} KB (${ratio}%)`
          );
          return compressed;
        })()
      );
    }
    return cache.get(filePath)!;
  }

  return {
    name: "glb-compress",

    configResolved(config) {
      isDev = config.command === "serve";
    },

    // Intercepts *.glb and *.glb?url before Vite's built-in asset handler
    async load(id) {
      const filePath = id.includes("?") ? id.slice(0, id.indexOf("?")) : id;
      if (!filePath.endsWith(".glb")) return null;

      this.addWatchFile(filePath);
      const compressed = await getCompressed(filePath);

      if (isDev) {
        // Hand the browser a URL pointing at our middleware
        const route = `${DEV_PREFIX}/${encodeURIComponent(filePath)}`;
        return `export default ${JSON.stringify(route)};`;
      }

      // Build: emit as a hashed asset and return its URL
      const refId = this.emitFile({
        type: "asset",
        name: filePath.split("/").pop()!,
        source: compressed,
      });
      return `export default import.meta.ROLLUP_FILE_URL_${refId};`;
    },

    // Dev: serve compressed GLBs on demand
    configureServer(server) {
      server.middlewares.use(DEV_PREFIX, async (req, res, next) => {
        // req.url is the part after DEV_PREFIX, e.g. /encoded%2Fabs%2Fpath.glb
        const filePath = decodeURIComponent(req.url!.slice(1));
        if (!filePath.endsWith(".glb")) return next();
        try {
          const compressed = await getCompressed(filePath);
          res.setHeader("Content-Type", "model/gltf-binary");
          res.setHeader("Cache-Control", "no-cache");
          res.end(compressed);
        } catch {
          next();
        }
      });
    },

    // Invalidate cache when the source file changes so HMR recompresses
    watchChange(id) {
      if (id.endsWith(".glb")) cache.delete(id);
    },
  };
}

