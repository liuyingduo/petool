import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const srcDir = path.join(root, 'src')
const distDir = path.join(root, 'dist')

fs.mkdirSync(distDir, { recursive: true })
fs.copyFileSync(path.join(srcDir, 'index.mjs'), path.join(distDir, 'index.mjs'))

console.log(`[browser-sidecar] built -> ${path.join(distDir, 'index.mjs')}`)
