<script setup lang="ts">
interface Band {
  name: string;
  bg: string;
}

const BAND_COLORS: Band[] = [
  { name: "Black",  bg: "#1a1a1a" },
  { name: "Brown",  bg: "#795548" },
  { name: "Red",    bg: "#e53935" },
  { name: "Orange", bg: "#fb8c00" },
  { name: "Yellow", bg: "#fdd835" },
  { name: "Green",  bg: "#43a047" },
  { name: "Blue",   bg: "#1e88e5" },
  { name: "Violet", bg: "#8e24aa" },
  { name: "Gray",   bg: "#757575" },
  { name: "White",  bg: "#f5f5f5" },
];

const GOLD: Band = { name: "Gold (±5%)", bg: "#ccaa00" };

const props = defineProps<{ value: string }>();

function parseOhms(value: string): number | null {
  const m = value.trim().match(/^(\d+(?:\.\d+)?)\s*([kKmMΩ]?)$/);
  if (!m) return null;
  const n = parseFloat(m[1]);
  switch (m[2].toLowerCase()) {
    case "k": return n * 1e3;
    case "m": return n * 1e6;
    default:  return n;
  }
}

function computeBands(value: string): Band[] {
  const ohms = parseOhms(value);
  if (ohms === null || ohms <= 0) return [];

  let exp = Math.floor(Math.log10(ohms)) - 1;
  let sig = Math.round(ohms / Math.pow(10, exp));
  if (sig >= 100) { sig = Math.round(sig / 10); exp++; }

  const d1 = Math.floor(sig / 10);
  const d2 = sig % 10;

  if (d1 > 9 || d2 > 9 || exp < 0 || exp > 9) return [];
  return [BAND_COLORS[d1], BAND_COLORS[d2], BAND_COLORS[exp], GOLD];
}

const bands = computeBands(props.value);
</script>

<template>
  <span
    v-if="bands.length"
    class="res-bands"
    :title="bands.map(b => b.name).join(' / ')"
  >
    <span
      v-for="(band, i) in bands"
      :key="i"
      class="band"
      :class="{ 'band-tol': i === 3 }"
      :style="{ background: band.bg }"
    />
  </span>
</template>

<style scoped>
.res-bands {
  display: inline-flex;
  gap: 1px;
  height: 1.1em;
  border-radius: 3px;
  overflow: hidden;
  border: 1px solid rgba(0, 0, 0, 0.35);
  background: rgba(0, 0, 0, 0.35);
  flex-shrink: 0;
  cursor: default;
}
.band {
  display: inline-block;
  width: 10px;
}
.band-tol {
  margin-left: 2px;
}
</style>
