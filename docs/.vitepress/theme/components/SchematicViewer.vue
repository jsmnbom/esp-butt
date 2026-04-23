<script setup lang="ts">
import { ref, watch } from "vue";
import { useData } from "vitepress";

const { isDark } = useData();
const svg = ref("");

async function load(dark: boolean) {
  const mod = dark
    ? await import("~/svg/schematic/dark.svg?raw")
    : await import("~/svg/schematic/light.svg?raw");
  svg.value = mod.default;
}

load(isDark.value);
watch(isDark, load);
</script>

<template>
  <div class="schematic-wrapper" v-html="svg" />
</template>

<style scoped>
.schematic-wrapper {
  width: 100%;
  height: 100%;
  border-top: 1px solid var(--vp-c-divider);
  overflow: auto;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  box-sizing: border-box;
}

.schematic-wrapper :deep(svg) {
  max-width: var(--vp-layout-max-width);
  max-height: calc(100dvh - var(--vp-nav-height) - 2rem);
  width: 100%;
  height: 100%;
}
</style>
