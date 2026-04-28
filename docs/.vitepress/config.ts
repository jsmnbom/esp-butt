import { defineConfig } from "vitepress";
import Icons from "unplugin-icons/vite";
import path from "node:path";
import fs from "node:fs";
import UnoCSS from 'unocss/vite'
import { fileURLToPath } from "node:url";
import { glbCompressPlugin } from "./plugins/glb";
import { ViteImageOptimizer } from "vite-plugin-image-optimizer";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  title: "esp-butt",
  description: "Bluetooth intimate hardware controller powered by buttplug.io",
  base: "/esp-butt/",

  themeConfig: {
    nav: [
      { text: "Home", link: "/" },
      { text: "Hardware", link: "/hardware/" },
      { text: "Firmware", link: "/firmware/" },
    ],

    sidebar: {
      "/hardware/": [
        {
          text: "Hardware",
          items: [
            { text: "Overview", link: "/hardware/" },
            { text: "Bill of Materials", link: "/hardware/bom" },
            { text: "Schematic", link: "/hardware/schematic" },
            { text: "PCB", link: "/hardware/pcb" },
            { text: "3D Models", link: "/hardware/models" },
          ],
        },
      ],
      "/firmware/": [
        {
          text: "Firmware",
          items: [
            { text: "Overview", link: "/firmware/" },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/jsmnbom/esp-butt" },
    ],

    search: {
      provider: 'local'
    }
  },

  vite: {
    logLevel: "info",
    resolve: {
      alias: [
        {
          find: /^.*\/VPFeature\.vue$/,
          replacement: fileURLToPath(
            new URL('./theme/components/CustomVPFeature.vue', import.meta.url)
          )
        },
        {
          find: /^~\/(.*)$/,
          replacement: path.resolve(__dirname, "../$1"),
        }
        
      ],
    },
    plugins: [
      glbCompressPlugin(),
      Icons({ compiler: "vue3", autoInstall: false }),
      {
        name: "ndjson",
        load(id) {
          if (!id.endsWith(".ndjson")) return null;
          const raw = fs.readFileSync(id, "utf-8");
          const events = raw.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
          return `export default ${JSON.stringify(events)};`;
        },
      },
      ViteImageOptimizer({
        svg: {
          floatPrecision: 2,
          plugins: [
            "preset-default",
            "removeDimensions",
            "removeTitle"
          ],
        },
      }),
      UnoCSS({
        configFile: path.resolve(__dirname, "uno.config.ts"),
      })
    ],
  },
});
