---
layout: home

hero:
  name: "esp-butt"
  text: "Bluetooth intimate hardware controller"
  tagline: Powered by buttplug.io — ESP32-S3 OLED display + rotary encoder + sliders
  actions:
    - theme: brand
      text: Hardware
      link: /hardware/
    - theme: alt
      text: Firmware
      link: /firmware/
    - theme: alt
      text: GitHub
      link: https://github.com/jsmnbom/esp-butt
---

<script setup>
import { withBase } from 'vitepress'
</script>

<ClientOnly>
  <HeroViewer
    :primaryUrl="withBase('/models/printed_parts.glb')"
    :secondaryUrl="withBase('/models/pcb.glb')"
    :recordingUrl="withBase('/models/recording.json')"
    :height="500"
  />
</ClientOnly>
