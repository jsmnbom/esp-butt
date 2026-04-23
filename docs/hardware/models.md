# 3D Models

<script setup>
import { withBase } from 'vitepress'
</script>

All parts are designed in [build123d](https://build123d.readthedocs.io/en/latest/) with custom helpers. Source notebooks are in the [`cad/`](https://github.com/jsmnbom/esp-butt/tree/main/cad) directory.

## Case

<StepDownload :files="['case_top', 'case_bottom']" />

<ClientOnly>
  <CadViewer :url="withBase('/models/case.glb')"/>
</ClientOnly>

## Encoder knob

<StepDownload files="encoder_knob" />

<ClientOnly>
  <CadViewer :url="withBase('/models/encoder_knob.glb')"/>
</ClientOnly>

## Slider knob

<StepDownload files="slider_knob" />

<ClientOnly>
  <CadViewer :url="withBase('/models/slider_knob.glb')"/>
</ClientOnly>

## Power switch cap

<StepDownload files="power_switch_cap" />

<ClientOnly>
  <CadViewer :url="withBase('/models/power_switch_cap.glb')"/>
</ClientOnly>


