<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { withBase } from "vitepress";
import FlagEU from "~icons/circle-flags/eu";
import FlagUS from "~icons/circle-flags/us";
import ResistorBands from "./ResistorBands.vue";

interface BomRow {
  [key: string]: string;
}

function displayValue(row: BomRow): string {
  const val = row["Value"] ?? "";
  const alt = row["Value_ALT"] ?? "";
  return alt ? `${val} / ${alt}` : val;
}

const rows = ref<BomRow[]>([]);
const headers = ref<string[]>([]);
const activeTab = ref<"EU" | "US">("EU");
const error = ref<string | null>(null);

function parseCSV(text: string): { headers: string[]; rows: BomRow[] } {
  const lines = text.trim().split("\n");
  if (lines.length < 2) return { headers: [], rows: [] };
  const hdrs = lines[0].split(";").map((h) => h.replace(/^"|"$/g, "").trim());
  const data = lines.slice(1).map((line) => {
    const values = line.split(";").map((v) => v.replace(/^"|"$/g, "").trim());
    return Object.fromEntries(hdrs.map((h, i) => [h, values[i] ?? ""]));
  });
  return { headers: hdrs, rows: data };
}

function distributorName(url: string): string {
  try {
    const host = new URL(url).hostname; // e.g. "www.digikey.dk"
    const parts = host.replace(/^www\./, "").split(".");
    // Drop TLD (last part) and any country-code SLD (last 2 chars = country code)
    const name = parts.length > 1 ? parts[0] : host;
    return name.charAt(0).toUpperCase() + name.slice(1);
  } catch {
    return "Buy";
  }
}

const sourceCol = computed(() =>
  activeTab.value === "EU" ? "Source_EU" : "Source_US"
);

const SELECTED_COLS = ["Reference", "Quantity", "Value"];

const visibleHeaders = computed(() =>
  headers.value.filter((h) => SELECTED_COLS.includes(h))
);

const hasSources = computed(
  () =>
    headers.value.includes("Source_EU") || headers.value.includes("Source_US")
);

onMounted(async () => {
  try {
    const resp = await fetch(withBase("/bom.csv"));
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const text = await resp.text();
    const parsed = parseCSV(text);
    headers.value = parsed.headers;
    rows.value = parsed.rows;
  } catch (e: any) {
    error.value = e.message;
  }
});
</script>

<template>
  <div class="bom-table">
    <p v-if="error" class="error">Failed to load BOM: {{ error }}</p>
    <template v-else-if="rows.length">
      <div v-if="hasSources" class="tabs">
        <button :class="{ active: activeTab === 'EU' }" @click="activeTab = 'EU'">
          <FlagEU style="font-size:1.2em" /> EU
        </button>
        <button :class="{ active: activeTab === 'US' }" @click="activeTab = 'US'">
          <FlagUS style="font-size:1.2em" /> US
        </button>
      </div>
      <table>
        <thead>
          <tr>
            <th v-for="h in visibleHeaders" :key="h">{{ h }}</th>
            <th v-if="hasSources">Source</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, i) in rows" :key="i">
            <td v-for="h in visibleHeaders" :key="h">
              <template v-if="h === 'Value'">
                <span class="value-cell">
                  {{ displayValue(row) }}
                  <ResistorBands :value="row['Value']" />
                </span>
              </template>
              <template v-else>{{ row[h] }}</template>
            </td>
            <td v-if="hasSources">
              <template v-if="row[sourceCol]">
                <template v-for="(url, ui) in row[sourceCol].split(',')" :key="ui">
                  <span v-if="ui > 0">, </span>
                  <a :href="url.trim()" target="_blank" rel="noopener noreferrer">
                    {{ distributorName(url.trim()) }}
                  </a>
                </template>
              </template>
              <span v-else>—</span>
            </td>
          </tr>
        </tbody>
      </table>
    </template>
    <p v-else>Loading BOM...</p>
  </div>
</template>

<style scoped>
.bom-table {
  overflow-x: auto;
}
.tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 12px;
}
.tabs button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 4px;
  background: transparent;
  color: var(--vp-c-text-1);
  cursor: pointer;
  font-size: 0.9em;
}
.tabs button.active {
  background: var(--vp-c-brand-1);
  color: white;
  border-color: var(--vp-c-brand-1);
}
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.9em;
}
th,
td {
  padding: 8px 12px;
  border: 1px solid var(--vp-c-divider);
  text-align: left;
}
th {
  background: var(--vp-c-bg-soft);
  font-weight: 600;
}
tr:nth-child(even) td {
  background: var(--vp-c-bg-soft);
}
.error {
  color: var(--vp-c-danger-1);
}
.value-cell {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
</style>
