# 3D Models

<script setup>
import {
  caseTopGlb, caseBottomGlb, encoderKnobGlb, sliderKnobGlb, powerSwitchCapGlb,
  caseTopStepUrl, caseBottomStepUrl, encoderKnobStepUrl,
  sliderKnobBodyStepUrl, sliderKnobInsertStepUrl, powerSwitchCapStepUrl,
} from '../.vitepress/theme/composables/models';
</script>

All parts are designed in [build123d](https://build123d.readthedocs.io/en/latest/) with custom helpers. Source notebooks are in the [`cad/`](https://github.com/jsmnbom/esp-butt/tree/main/cad) directory.

## Printing

The models are designed around my **Voron V2.4** printer. Other printers should work, but tolerances on snap-fits and clearance holes are tuned to my machine, so some adjustment may be needed.

**Material:** ABS is recommended for durability and dimensional stability. I use filament from [TM3D Filament](https://tm3dfilament.com/) (no affiliation).

**Slicer:** All parts were sliced with **OrcaSlicer**.

**General settings:** 0.15 mm layer height · 3 wall loops · 15–25% gyroid infill.

**Orientation:** All parts should be printed in the orientation in the step files.

## Case

2 wall loops is sufficient. Set outer wall width to **0.4 mm** — required for the slicer to handle the antenna cutout geometry correctly. Seam position and seam scarf/gap settings may need to be tuned to hide the seam on visible faces.

### Case Top

<StepDownload :files="{ name: 'case_top', url: caseTopStepUrl }" />

<ClientOnly>
  <CadViewer :url="caseTopGlb"/>
</ClientOnly>

### Case Bottom

<StepDownload :files="{ name: 'case_bottom', url: caseBottomStepUrl }" />

<ClientOnly>
  <CadViewer :url="caseBottomGlb"/>
</ClientOnly>

## Encoder knob

1 wall loop. Use a **concentric** top surface pattern to give the concave top a smooth feel.

<StepDownload :files="{ name: 'encoder_knob', url: encoderKnobStepUrl }" />

<ClientOnly>
  <CadViewer :url="encoderKnobGlb"/>
</ClientOnly>

## Slider knob

**Body:** 1 wall loop. Use an **aligned rectilinear** top surface pattern with infill direction set to **0°** — this makes the knob divots noticeably smoother.

**Inserts:** Print flat. Use **0.12 mm layer height** and a very slow print speed — these are small precision parts and quality matters here.

<StepDownload :files="[{ name: 'slider_knob_body', url: sliderKnobBodyStepUrl }, { name: 'slider_knob_insert', url: sliderKnobInsertStepUrl }]" />

<ClientOnly>
  <CadViewer :url="sliderKnobGlb"/>
</ClientOnly>

## Power switch cap

General settings apply.

<StepDownload :files="{ name: 'power_switch_cap', url: powerSwitchCapStepUrl }" />

<ClientOnly>
  <CadViewer :url="powerSwitchCapGlb"/>
</ClientOnly>


