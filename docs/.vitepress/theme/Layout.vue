<script setup lang="ts">
import DefaultTheme from "vitepress/theme";
import { useData, withBase } from "vitepress";
import HeroImage from "./components/HeroImage.vue";

const { Layout } = DefaultTheme;
const { frontmatter } = useData();
</script>

<template>
  <Layout>
    <template v-if="frontmatter.layout === 'home'" #home-hero-image>
      <HeroImage />
    </template>
    <template #layout-bottom>
      <footer class="esp-butt-footer">
        Some links on this site may include referral or promotional codes.
      </footer>
    </template>
  </Layout>
</template>
