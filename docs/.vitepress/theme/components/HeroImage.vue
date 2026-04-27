<script lang="ts" setup>
import { assemblyGlb, pcbGlb } from "../composables/models";

const modelUrls = [assemblyGlb, pcbGlb];
</script>

<template>
  <ClientOnly>
    <HeroViewer :model-urls="modelUrls" />
  </ClientOnly>
</template>
