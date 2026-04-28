<script setup lang="ts">
</script>

<template>
  <div class="pcb-grid">
    <span class="pcb-label front-label">Front</span>
    <div class="pcb-divider" />
    <span class="pcb-label back-label">Back</span>
    <div class="pcb-svg front-svg">
      <img src="" class="pcb-img pcb-front" alt="PCB Front">
    </div>
    <div class="pcb-svg back-svg">
      <img src="" class="pcb-img pcb-back" alt="PCB Back">
    </div>
  </div>
</template>

<style scoped>
.pcb-grid {
  width: 100%;
  max-width: var(--vp-layout-max-width);
  margin: 0 auto;
  display: grid;
  grid-template-columns: 1fr 1px 1fr;
  grid-template-rows: auto auto;
  grid-template-areas:
    "front-label divider back-label"
    "front-svg   divider back-svg";
  overflow: hidden;
  box-sizing: border-box;
}

.pcb-label {
  font-size: 0.75rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--vp-c-text-2);
  padding: 0.5rem 1rem;
  align-self: center;
  justify-self: center;
}

.front-label { grid-area: front-label; }
.back-label  { grid-area: back-label; }

.pcb-divider {
  grid-area: divider;
  background: var(--vp-c-divider);
}

.pcb-svg {
  overflow: hidden;
  padding: 1rem;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  align-self: flex-end;
}

.front-svg { grid-area: front-svg; }
.back-svg  { grid-area: back-svg; }

.pcb-img {
  max-width: 100%;
  width: 100%;
  height: auto;
}
</style>

<style>
.pcb-front {
  content: url('/svg/front/light.svg');
}

.pcb-back {
  content: url('/svg/back/light.svg');
}

html.dark .pcb-front {
  content: url('/svg/front/dark.svg');
}

html.dark .pcb-back {
  content: url('/svg/back/dark.svg');
}
</style>
