import { defineConfig } from "vitepress";
import Icons from "unplugin-icons/vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
  },

  vue: {
    template: {
      compilerOptions: {
        isCustomElement: (tag: string) => tag.startsWith("Tres") && tag !== "TresCanvas",
      },
    },
  },

  vite: {
    resolve: {
      alias: {
        "~/svg": path.resolve(__dirname, "../svg"),
      },
    },
    plugins: [
      Icons({ compiler: "vue3", autoInstall: false }),
    ],
  },
});
