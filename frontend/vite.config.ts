import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// declare module "*.css";
// declare module "@fontsource/*" {}
// declare module "@fontsource-variable/*" {}

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
})
