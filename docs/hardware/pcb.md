# PCB

PCB design files are available in the [`pcb/`](https://github.com/jsmnbom/esp-butt/tree/main/pcb) directory. The PCB was designed using KiCad, and the source files are included.

<PCBViewer />

## 3D Preview

<script setup>
import { withBase } from 'vitepress'
</script>

<ClientOnly>
  <CadViewer :url="withBase('/models/pcb.glb')"/>
</ClientOnly>
