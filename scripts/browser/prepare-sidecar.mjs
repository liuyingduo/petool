import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const repoRoot = path.resolve(__dirname, '..', '..')
const sidecarRoot = path.join(repoRoot, 'browser-sidecar')
const tauriRoot = path.join(repoRoot, 'src-tauri')
const resourcesRoot = path.join(tauriRoot, 'resources')
const sidecarResourceRoot = path.join(resourcesRoot, 'browser-sidecar')
const binariesRoot = path.join(tauriRoot, 'binaries')

function run(command, args, opts = {}) {
  const executable =
    process.platform === 'win32' && !command.endsWith('.cmd') && ['npm', 'npx'].includes(command)
      ? `${command}.cmd`
      : command
  const result = spawnSync(executable, args, {
    stdio: 'inherit',
    shell: false,
    ...opts
  })
  if (result.status !== 0) {
    throw new Error(`Command failed: ${executable} ${args.join(' ')}`)
  }
}

function detectTargetTriple() {
  if (process.platform === 'win32' && process.arch === 'x64') return 'x86_64-pc-windows-msvc'
  if (process.platform === 'darwin' && process.arch === 'arm64') return 'aarch64-apple-darwin'
  if (process.platform === 'darwin' && process.arch === 'x64') return 'x86_64-apple-darwin'
  if (process.platform === 'linux' && process.arch === 'x64') return 'x86_64-unknown-linux-gnu'
  if (process.platform === 'linux' && process.arch === 'arm64') return 'aarch64-unknown-linux-gnu'
  throw new Error(`Unsupported platform/arch: ${process.platform}/${process.arch}`)
}

function cleanAndCopyDir(srcDir, destDir) {
  fs.rmSync(destDir, { recursive: true, force: true })
  fs.mkdirSync(destDir, { recursive: true })
  fs.cpSync(srcDir, destDir, { recursive: true })
}

function sha256OfFile(filePath) {
  if (!fs.existsSync(filePath)) return null
  const content = fs.readFileSync(filePath)
  return createHash('sha256').update(content).digest('hex')
}

function verifySidecarConsistency() {
  const srcEntry = path.join(sidecarRoot, 'src', 'index.mjs')
  const distEntry = path.join(sidecarRoot, 'dist', 'index.mjs')
  const resourceEntry = path.join(sidecarResourceRoot, 'dist', 'index.mjs')
  const srcHash = sha256OfFile(srcEntry)
  const distHash = sha256OfFile(distEntry)
  const resourceHash = sha256OfFile(resourceEntry)

  if (!srcHash || !distHash || !resourceHash) {
    console.warn('[prepare-sidecar] warning: sidecar consistency check skipped (missing file)')
    return
  }

  const mismatches = []
  if (srcHash !== distHash) {
    mismatches.push('browser-sidecar/src/index.mjs != browser-sidecar/dist/index.mjs')
  }
  if (distHash !== resourceHash) {
    mismatches.push('browser-sidecar/dist/index.mjs != src-tauri/resources/browser-sidecar/dist/index.mjs')
  }

  if (mismatches.length > 0) {
    console.warn('[prepare-sidecar] warning: sidecar file hashes are inconsistent')
    for (const line of mismatches) {
      console.warn(`  - ${line}`)
    }
    return
  }

  console.log('[prepare-sidecar] sidecar hash check passed')
}

function copyBundledNode() {
  fs.mkdirSync(binariesRoot, { recursive: true })
  const triple = detectTargetTriple()
  const ext = process.platform === 'win32' ? '.exe' : ''
  const targetName = `browser-node-${triple}${ext}`
  const targetPath = path.join(binariesRoot, targetName)
  fs.copyFileSync(process.execPath, targetPath)
  if (process.platform !== 'win32') {
    fs.chmodSync(targetPath, 0o755)
  }
  console.log(`[prepare-sidecar] bundled node runtime: ${targetPath}`)
}

function main() {
  console.log('[prepare-sidecar] installing sidecar dependencies')
  run('npm', ['install'], { cwd: sidecarRoot })

  console.log('[prepare-sidecar] building sidecar')
  run('npm', ['run', 'build'], { cwd: sidecarRoot })

  console.log('[prepare-sidecar] copying sidecar resources')
  cleanAndCopyDir(path.join(sidecarRoot, 'dist'), path.join(sidecarResourceRoot, 'dist'))
  verifySidecarConsistency()

  if (process.env.PETOOL_BROWSER_SKIP_NODE !== '1') {
    copyBundledNode()
  }

  console.log('[prepare-sidecar] done')
}

main()
