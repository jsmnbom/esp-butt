<script setup lang="ts">
import IconDownload from "~icons/mdi/download";

export interface StepFile {
  name: string;
  url: string;
}

const props = defineProps<{
  files: StepFile | StepFile[];
}>();

const fileList = Array.isArray(props.files) ? props.files : [props.files];
</script>

<template>
  <div class="step-downloads">
    <a
      v-for="file in fileList"
      :key="file.name"
      :href="file.url"
      :download="`${file.name}.step`"
      class="download-btn"
    ><IconDownload style="font-size:1em;flex-shrink:0" /> {{ file.name }}.step</a>
  </div>
</template>

<style scoped>
.step-downloads {
  margin: 8px 0 4px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.download-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  font-size: 0.875em;
  text-decoration: none !important;
  color: var(--vp-c-text-1) !important;
  background: var(--vp-c-bg);
  transition: background 0.2s;
}
.download-btn:hover {
  background: var(--vp-c-bg-soft);
}
</style>
