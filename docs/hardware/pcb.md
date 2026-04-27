# PCB

PCB design files are available in the [`pcb/`](https://github.com/jsmnbom/esp-butt/tree/main/pcb) directory. The PCB was designed using KiCad, and the source files are included.

<PCBViewer />

## 3D Preview

<script setup>
import { pcbGlb } from '../.vitepress/theme/composables/models';
</script>

<ClientOnly>
  <CadViewer :url="pcbGlb"/>
</ClientOnly>
