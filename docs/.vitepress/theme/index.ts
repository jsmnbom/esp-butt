import DefaultTheme from "vitepress/theme";
import Layout from "./Layout.vue";
import { defineAsyncComponent } from "vue";
import BomTable from "./components/BomTable.vue";
import StepDownload from "./components/StepDownload.vue";
import SchematicViewer from "./components/SchematicViewer.vue";
import PCBViewer from "./components/PCBViewer.vue";
import type { Theme } from "vitepress";
import './custom.css'
import 'virtual:uno.css'

export default {
  extends: DefaultTheme,
  Layout,
  enhanceApp({ app }) {
    app.component("CadViewer", defineAsyncComponent(() => import("./components/CadViewer.vue")));
    app.component("HeroViewer", defineAsyncComponent(() => import("./components/HeroViewer.vue")));
    app.component("BomTable", BomTable);
    app.component("StepDownload", StepDownload);
    app.component("SchematicViewer", SchematicViewer);
    app.component("PCBViewer", PCBViewer);
  },
} satisfies Theme;
