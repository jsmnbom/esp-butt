declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<{}, {}, any>;
  export default component;
}

declare module "*?inline" {
  const content: string;
  export default content;
}

declare module "*.svg" {
  const url: string;
  export default url;
}

declare module "~icons/*" {
  import type { FunctionalComponent, SVGAttributes } from "vue";
  const component: FunctionalComponent<SVGAttributes>;
  export default component;
}

declare module "*.css" {
  const content: string;
  export default content;
}

declare module "*.ndjson" {
  import type { RecordingEvent } from "./theme/composables/useRecording";
  const events: RecordingEvent[];
  export default events;
}

declare module "*.glb" {
  const url: string;
  export default url;
}

declare module "*.step?url" {
  const url: string;
  export default url;
}

declare module "*.gif?url" {
  const url: string;
  export default url;
}

declare module "*.csv?raw" {
  const content: string;
  export default content;
}
