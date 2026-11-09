/**
 * Extract Slate's upstream fixture modules into JSON that the Rust test suite
 * can replay.
 *
 * The script intentionally reads Slate's TypeScript sources straight from a
 * pinned tarball, so the generated fixtures describe exactly one upstream
 * commit. `tsx` only transpiles; it does not type-check the fixture files.
 *
 * Usage:
 *   npm install
 *   npm run extract
 *
 * Environment:
 *   UPSTREAM_SHA    override the pinned commit
 *   UPSTREAM_CACHE  override the tarball/work cache (defaults to ~/.cache)
 */

import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const DEFAULT_SHA = 'fd42a8046db8da329f4584dcf2a68ac7abd3137b'
const SHA = process.env.UPSTREAM_SHA ?? DEFAULT_SHA
const HERE = path.dirname(fileURLToPath(import.meta.url))
const REPO = path.resolve(HERE, '../..')
const CACHE =
  process.env.UPSTREAM_CACHE ??
  path.join(os.homedir(), '.cache', 'leptoslate-upstream')
const SOURCE = path.join(CACHE, `slate-${SHA}`)
const TEST_ROOT = path.join(SOURCE, 'packages/slate/test')
const WORK = path.join(CACHE, `work-${SHA}`)
const OUT = path.join(
  REPO,
  'crates/leptoslate-core/tests/upstream/fixtures'
)
const CATEGORIES = [
  'interfaces/Path',
  'interfaces/Point',
  'interfaces/Range',
  'operations',
] as const

type TraceCall = {
  target: string
  method: string
  args: unknown[]
}

const trace: TraceCall[] = []
let depth = 0

/**
 * Values inside fixtures are plain JSON plus `undefined` (from optional
 * operation properties). Rust only needs the JSON shape, so normalize the
 * handful of JavaScript-only values explicitly.
 */
function toJson(value: unknown): unknown {
  if (value === undefined) return null
  if (typeof value === 'number' && !Number.isFinite(value)) return null
  if (Array.isArray(value)) return value.map(toJson)
  if (value !== null && typeof value === 'object') {
    const result: Record<string, unknown> = {}
    for (const key of Object.keys(value as Record<string, unknown>)) {
      result[key] = toJson((value as Record<string, unknown>)[key])
    }
    return result
  }
  return value
}

function ensureSource() {
  if (fs.existsSync(path.join(SOURCE, 'packages/slate/src/index.ts'))) return

  fs.mkdirSync(CACHE, { recursive: true })
  const tarball = path.join(CACHE, `slate-${SHA}.tar.gz`)
  if (!fs.existsSync(tarball)) {
    console.log(`downloading Slate ${SHA}…`)
    execFileSync(
      'curl',
      [
        '--fail',
        '--location',
        '--silent',
        '--show-error',
        '--output',
        tarball,
        `https://codeload.github.com/ianstormtaylor/slate/tar.gz/${SHA}`,
      ],
      { stdio: 'inherit' }
    )
  }

  fs.mkdirSync(SOURCE, { recursive: true })
  execFileSync('tar', ['xzf', tarball, '--strip-components=1', '-C', SOURCE], {
    stdio: 'inherit',
  })
}

/**
 * Create tiny ESM packages that re-export Slate's TypeScript sources, so the
 * fixtures resolve `slate` and `slate-hyperscript` without a build step.
 */
function ensureDeps() {
  const nodeModules = path.join(HERE, 'node_modules')
  const deps = path.join(HERE, '.deps')
  fs.mkdirSync(nodeModules, { recursive: true })
  fs.mkdirSync(deps, { recursive: true })

  for (const name of ['slate', 'slate-hyperscript']) {
    const packageDir = path.join(deps, name)
    fs.mkdirSync(packageDir, { recursive: true })
    fs.writeFileSync(
      path.join(packageDir, 'package.json'),
      JSON.stringify(
        {
          name,
          private: true,
          type: 'module',
          exports: { '.': './index.ts' },
        },
        null,
        2
      )
    )
    const source = path.join(SOURCE, 'packages', name, 'src/index.ts')
    fs.writeFileSync(
      path.join(packageDir, 'index.ts'),
      `export * from ${JSON.stringify(source)}\n`
    )

    const link = path.join(nodeModules, name)
    fs.rmSync(link, { recursive: true, force: true })
    fs.symlinkSync(packageDir, link, 'dir')

    // Slate's own sources resolve their bare imports from the extracted
    // repository, not from this script directory.
    const sourceLink = path.join(SOURCE, 'node_modules', name)
    fs.mkdirSync(path.dirname(sourceLink), { recursive: true })
    fs.rmSync(sourceLink, { recursive: true, force: true })
    fs.symlinkSync(packageDir, sourceLink, 'dir')
  }
}

/**
 * Rewritten fixtures live in the cache, so give them their own `node_modules`
 * that points at the generated Slate source packages.
 */
function linkWorkDeps() {
  const nodeModules = path.join(WORK, 'node_modules')
  fs.mkdirSync(nodeModules, { recursive: true })
  for (const name of ['slate', 'slate-hyperscript']) {
    const link = path.join(nodeModules, name)
    fs.rmSync(link, { recursive: true, force: true })
    fs.symlinkSync(path.join(HERE, '.deps', name), link, 'dir')
  }
}

function writeShim() {
  const shim = path.join(WORK, '__shim.tsx')
  fs.mkdirSync(WORK, { recursive: true })
  fs.writeFileSync(
    shim,
    `/** @jsx jsx */
import { createEditor as createBaseEditor } from 'slate'
import {
  createHyperscript,
  createEditor as createEditorCreator,
} from 'slate-hyperscript'

const withTest = (editor: any) => {
  const { isInline, isVoid, isElementReadOnly, isSelectable } = editor
  editor.isInline = (element: any) =>
    element.inline === true ? true : isInline(element)
  editor.isVoid = (element: any) =>
    element.void === true ? true : isVoid(element)
  editor.isElementReadOnly = (element: any) =>
    element.readOnly === true ? true : isElementReadOnly(element)
  editor.isSelectable = (element: any) =>
    element.nonSelectable === true ? false : isSelectable(element)
  return editor
}

export const jsx = createHyperscript({
  elements: {
    block: {},
    inline: { inline: true },
  },
  creators: {
    editor: createEditorCreator(() => withTest(createBaseEditor())),
  },
})
`
  )
  return shim
}

function rewriteFixture(sourceFile: string, shim: string) {
  const relative = path.relative(TEST_ROOT, sourceFile)
  const target = path.join(WORK, relative)
  fs.mkdirSync(path.dirname(target), { recursive: true })

  const from = path
    .relative(path.dirname(target), shim)
    .split(path.sep)
    .join('/')
  const specifier = from.startsWith('.') ? from : `./${from}`

  const source = fs
    .readFileSync(sourceFile, 'utf8')
    // `from '../..'`, `from '../../..'` all point at Slate's test harness
    // index. Replace just those import specifiers with our local shim.
    .replace(
      /from\s+(['"])(?:\.\.\/)+(?:\.\.)?\1/g,
      `from '${specifier}'`
    )

  fs.writeFileSync(target, source)
  return target
}

function installInstrumentation(slate: any) {
  for (const target of ['Path', 'Point', 'Range'] as const) {
    const object = slate[target]
    for (const method of Object.keys(object)) {
      const original = object[method]
      if (typeof original !== 'function') continue
      object[method] = function (...args: unknown[]) {
        if (depth > 0) return original.apply(this, args)

        depth += 1
        try {
          let result = original.apply(this, args)
          // Fixtures consume `Range.points` with `Array.from`, so materialize
          // the generator and record a JSON-friendly result.
          if (
            result &&
            typeof result === 'object' &&
            typeof result.next === 'function'
          ) {
            result = Array.from(result)
          }
          trace.push({ target, method, args: args.map(toJson) })
          return result
        } finally {
          depth -= 1
        }
      }
    }
  }
}

function fixtureFiles(category: string) {
  const root = path.join(TEST_ROOT, category)
  const files: string[] = []

  const walk = (directory: string) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const full = path.join(directory, entry.name)
      if (entry.isDirectory()) walk(full)
      else if (entry.isFile() && entry.name.endsWith('.tsx')) files.push(full)
    }
  }

  walk(root)
  return files.sort()
}

function writeFixture(source: string, data: unknown) {
  const relative = path.relative(TEST_ROOT, source).replace(/\.tsx$/, '.json')
  const target = path.join(OUT, relative)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, `${JSON.stringify(data, null, 2)}\n`)
}

async function main() {
  if (!fs.existsSync(path.join(TEST_ROOT, 'index.ts'))) {
    ensureSource()
  }
  ensureDeps()

  fs.rmSync(OUT, { recursive: true, force: true })
  fs.mkdirSync(OUT, { recursive: true })
  fs.rmSync(WORK, { recursive: true, force: true })
  fs.mkdirSync(WORK, { recursive: true })
  linkWorkDeps()
  const shim = writeShim()

  // `tsx` transpiles Slate's extensionless TypeScript imports to CommonJS.
  // Its dynamic-import namespace therefore nests the real exports under
  // `module.exports`; unwrap that so instrumentation sees `Path`/`Point`/`Range`.
  const importedSlate: any = await import('slate')
  const slate = importedSlate.Path
    ? importedSlate
    : (importedSlate['module.exports'] ?? importedSlate.default ?? importedSlate)
  installInstrumentation(slate)

  const skipped: { source: string; reason: string }[] = []
  const counts: Record<string, number> = {}
  let imported = 0

  for (const category of CATEGORIES) {
    counts[category] = 0
    for (const sourceFile of fixtureFiles(category)) {
      const relative = path.relative(TEST_ROOT, sourceFile)
      let module: any

      try {
        module = await import(
          `${pathToFileURL(rewriteFixture(sourceFile, shim)).href}?v=${imported++}`
        )
      } catch (error) {
        skipped.push({ source: relative, reason: `import failed: ${error}` })
        continue
      }

      // CommonJS interop again: the fixture's exports can be nested.
      if (
        !('test' in module) &&
        !('input' in module) &&
        !('operations' in module)
      ) {
        module = module['module.exports'] ?? module.default ?? module
      }

      try {
        if (category === 'operations') {
          const input = module.input
          const output = module.output
          assert.ok(input?.children, `${relative}: missing input editor`)
          assert.ok(output?.children, `${relative}: missing output editor`)
          assert.ok(
            Array.isArray(module.operations),
            `${relative}: missing operations`
          )

          writeFixture(sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: `packages/slate/test/${relative}`,
            },
            kind: 'operations',
            input: {
              children: toJson(input.children),
              selection: toJson(input.selection ?? null),
            },
            operations: toJson(module.operations),
            output: {
              children: toJson(output.children),
              selection: toJson(output.selection ?? null),
            },
          })
        } else {
          trace.length = 0
          const result = module.test(module.input)
          assert.deepEqual(
            result,
            module.output,
            `${relative}: fixture's own test/input/output disagree`
          )
          assert.ok(
            trace.length > 0,
            `${relative}: no instrumented Path/Point/Range call`
          )

          writeFixture(sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: `packages/slate/test/${relative}`,
            },
            kind: 'interface',
            ...(module.input === undefined ? {} : { input: toJson(module.input) }),
            calls: trace.map(call => ({
              target: call.target,
              method: call.method,
              args: call.args,
            })),
            output: toJson(module.output),
          })
        }
        counts[category] += 1
      } catch (error) {
        skipped.push({ source: relative, reason: String(error) })
      }
    }
  }

  fs.writeFileSync(
    path.join(OUT, '..', 'skipped.json'),
    `${JSON.stringify(
      {
        upstream: {
          repository: 'ianstormtaylor/slate',
          commit: SHA,
        },
        skipped: skipped.sort((a, b) => a.source.localeCompare(b.source)),
      },
      null,
      2
    )}\n`
  )

  console.log(`upstream commit: ${SHA}`)
  for (const category of CATEGORIES) {
    console.log(`${category}: ${counts[category]}`)
  }
  console.log(`skipped: ${skipped.length}`)
  for (const entry of skipped) {
    console.log(`  - ${entry.source}: ${entry.reason.split('\n')[0]}`)
  }
}

await main()
