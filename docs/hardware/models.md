# 3D Models

<script setup>
import {
  caseGlb, encoderKnobGlb, sliderKnobGlb, powerSwitchCapGlb,
  caseTopStepUrl, caseBottomStepUrl, encoderKnobStepUrl,
  sliderKnobStepUrl, powerSwitchCapStepUrl,
} from '../.vitepress/theme/composables/models';
</script>

All parts are designed in [build123d](https://build123d.readthedocs.io/en/latest/) with custom helpers. Source notebooks are in the [`cad/`](https://github.com/jsmnbom/esp-butt/tree/main/cad) directory.

## Case

<StepDownload :files="[{ name: 'case_top', url: caseTopStepUrl }, { name: 'case_bottom', url: caseBottomStepUrl }]" />

<ClientOnly>
  <CadViewer :url="caseGlb"/>
</ClientOnly>

## Encoder knob

<StepDownload :files="{ name: 'encoder_knob', url: encoderKnobStepUrl }" />

<ClientOnly>
  <CadViewer :url="encoderKnobGlb"/>
</ClientOnly>

## Slider knob

<StepDownload :files="{ name: 'slider_knob', url: sliderKnobStepUrl }" />

<ClientOnly>
  <CadViewer :url="sliderKnobGlb"/>
</ClientOnly>

## Power switch cap

<StepDownload :files="{ name: 'power_switch_cap', url: powerSwitchCapStepUrl }" />

<ClientOnly>
  <CadViewer :url="powerSwitchCapGlb"/>
</ClientOnly>


