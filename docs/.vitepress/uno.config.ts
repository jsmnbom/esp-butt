import { defineConfig, type Extractor } from 'unocss'
import { presetIcons } from 'unocss'

const extractorIconYaml: Extractor = {
  name: 'extractor-icon-yaml',
  extract({ code }) {
    return code.match(/\bi-[\w-]+:[\w-]+\b/g) ?? []
  }
}

export default defineConfig({
  content: {
    pipeline: {
      include: [/\.md$/]
    }
  },
  extractors: [extractorIconYaml],
  presets: [
    presetIcons({})
  ]
})
