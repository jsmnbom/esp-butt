# PCB

PCB design files are available in the [`pcb/`](https://github.com/jsmnbom/esp-butt/tree/main/pcb) directory. The PCB was designed using KiCad, and the source files are included.

<PCBViewer />

## Manufacturing

The PCBs for this project were manufactured at [Aisler](https://aisler.net). They are a European fab, I went with them for their strong focus on hobbyist and maker customers, good reviews for quality and support, and convenient shipping options.

Aisler also offers two stackable discounts worth knowing about:

- **LOGO Hack** — a checkbox during checkout that grants a 25% discount in exchange for the Aisler logo being placed on the board (the PCB already has a designated area for this).
- **Part Professional program** — when boards include parts from specific vendors you get a discount on the PCB cost. For this PCB the rotary encoder and 1x4 pin socket header can be sourced from Würth Elektronik which take advantage of this program.

With these combined, 3 PCBs including fast shipping and the KiCad sponsor contribution comes to around **€35** at the time of writing.

Alternative fabs such as JLCPCB or PCBWay may offer lower prices, but I have not tested them with this specific board. They may require adjustments to export settings and may have longer turnaround times depending on shipping options. However the board overall is quite simple and should be compatible with a wide range of manufacturers.

## 3D Preview

<script setup>
import { pcbGlb } from '../.vitepress/theme/composables/models';
</script>

<ClientOnly>
  <CadViewer :url="pcbGlb"/>
</ClientOnly>
