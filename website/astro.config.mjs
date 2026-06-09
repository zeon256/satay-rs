// @ts-check

import tailwindcss from "@tailwindcss/vite"
import react from "@astrojs/react"
import { defineConfig } from "astro/config"

// https://astro.build/config
export default defineConfig({
  // Set SITE_URL when building for production so og:url and og:image resolve correctly.
  site: process.env.SITE_URL,
  vite: {
    plugins: [tailwindcss()],
  },
  integrations: [react()],
})
