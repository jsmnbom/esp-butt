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
  </Layout>
</template>
