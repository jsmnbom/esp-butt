import DefaultTheme from "vitepress/theme";
import CadViewer from "./components/CadViewer.vue";
import HeroViewer from "./components/HeroViewer.vue";
import BomTable from "./components/BomTable.vue";
import StepDownload from "./components/StepDownload.vue";
import SchematicViewer from "./components/SchematicViewer.vue";
import PCBViewer from "./components/PCBViewer.vue";
import type { Theme } from "vitepress";
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component("CadViewer", CadViewer);
    app.component("HeroViewer", HeroViewer);
    app.component("BomTable", BomTable);
    app.component("StepDownload", StepDownload);
    app.component("SchematicViewer", SchematicViewer);
    app.component("PCBViewer", PCBViewer);
  },
} satisfies Theme;
