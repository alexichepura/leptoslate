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
const SLATE_TEST_ROOT = path.join(SOURCE, 'packages/slate/test')
const HISTORY_TEST_ROOT = path.join(SOURCE, 'packages/slate-history/test')
const WORK = path.join(CACHE, `work-${SHA}`)
const OUT = path.join(
  REPO,
  'crates/leptoslate-core/tests/upstream/fixtures'
)

type Category = {
  /** Prefix used for output paths and known-failure keys. */
  name: string
  root: string
  /** Directory relative to `root`. */
  dir: string
}

const slateCategory = (dir: string): Category => ({
  name: dir,
  root: SLATE_TEST_ROOT,
  dir,
})

const CATEGORIES: Category[] = [
  slateCategory('interfaces/Path'),
  slateCategory('interfaces/Point'),
  slateCategory('interfaces/Range'),
  slateCategory('operations'),
  slateCategory('transforms/insertText'),
  slateCategory('transforms/splitNodes'),
  slateCategory('transforms/mergeNodes'),
  slateCategory('transforms/removeNodes'),
  slateCategory('transforms/insertNodes'),
  slateCategory('transforms/delete'),
  slateCategory('transforms/setNodes'),
  slateCategory('transforms/unsetNodes'),
  slateCategory('transforms/moveNodes'),
  slateCategory('transforms/insertFragment'),
  slateCategory('transforms/move'),
  slateCategory('transforms/wrapNodes'),
  slateCategory('transforms/unwrapNodes'),
  slateCategory('transforms/liftNodes'),
  slateCategory('transforms/select'),
  slateCategory('transforms/deselect'),
  slateCategory('transforms/setPoint'),
  slateCategory('transforms/setChildren'),
  slateCategory('transforms/general'),
  slateCategory('normalization'),
  { name: 'history/undo', root: HISTORY_TEST_ROOT, dir: 'undo' },
]

type TraceCall = {
  target: string
  method: string
  args: unknown[]
}

const trace: TraceCall[] = []
let depth = 0
// Some fixtures configure the editor inside `run` (for example by assigning
// `editor.children` or overriding `isSelectable`). Capture the document at the
// first instrumented call so the replay starts from the state the transform
// actually saw.
let callInputSnapshot: {
  children: unknown
  selection: unknown
} | null = null

/**
 * Values inside fixtures are plain JSON plus `undefined` (from optional
 * operation properties). Rust only needs the JSON shape, so normalize the
 * handful of JavaScript-only values explicitly.
 */
function toJson(value: unknown): unknown {
  if (value === undefined) return null
  if (typeof value === 'function') return { __fn: value.toString() }
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

  for (const name of ['slate', 'slate-hyperscript', 'slate-history']) {
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

  // Some upstream fixtures and Slate sources import lodash directly.
  const lodash = path.join(HERE, 'node_modules', 'lodash')
  for (const root of [SOURCE, WORK]) {
    const link = path.join(root, 'node_modules', 'lodash')
    fs.mkdirSync(path.dirname(link), { recursive: true })
    fs.rmSync(link, { recursive: true, force: true })
    fs.symlinkSync(lodash, link, 'dir')
  }
}

/**
 * Rewritten fixtures live in the cache, so give them their own `node_modules`
 * that points at the generated Slate source packages.
 */
function linkWorkDeps() {
  const nodeModules = path.join(WORK, 'node_modules')
  fs.mkdirSync(nodeModules, { recursive: true })
  for (const name of ['slate', 'slate-hyperscript', 'slate-history']) {
    const link = path.join(nodeModules, name)
    fs.rmSync(link, { recursive: true, force: true })
    fs.symlinkSync(path.join(HERE, '.deps', name), link, 'dir')
  }
  const lodash = path.join(nodeModules, 'lodash')
  fs.rmSync(lodash, { recursive: true, force: true })
  fs.symlinkSync(path.join(HERE, 'node_modules', 'lodash'), lodash, 'dir')
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
import { withHistory } from 'slate-history'

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
    editor: createEditorCreator(() =>
      withTest(withHistory(createBaseEditor()))
    ),
  },
})
`
  )
  return shim
}

function outputRelative(category: Category, sourceFile: string) {
  return path.join(
    category.name,
    path.relative(path.join(category.root, category.dir), sourceFile)
  )
}

function rewriteFixture(
  sourceFile: string,
  shim: string,
  category: Category
) {
  const target = path.join(WORK, outputRelative(category, sourceFile))
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
  for (const target of [
    'Path',
    'Point',
    'Range',
    'Transforms',
    'Editor',
  ] as const) {
    const object = slate[target]
    for (const method of Object.keys(object)) {
      // `withoutNormalizing` runs a callback; wrapping it would hide the
      // Transforms call inside. Other Editor helpers are recorded because
      // history fixtures call them directly.
      if (target === 'Editor' && method === 'withoutNormalizing') continue
      const original = object[method]
      if (typeof original !== 'function') continue
      object[method] = function (...args: unknown[]) {
        if (depth > 0) return original.apply(this, args)

        if (
          callInputSnapshot == null &&
          (target === 'Transforms' || target === 'Editor') &&
          args[0] &&
          typeof args[0] === 'object' &&
          'children' in (args[0] as object)
        ) {
          const editorArg = args[0] as any
          callInputSnapshot = convertSnapshot({
            children: toJson(editorArg.children),
            selection: toJson(editorArg.selection ?? null),
          })
        }

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
          // Editor-first APIs (Transforms/Editor) record only the arguments
          // after the editor itself.
          const recorded =
            target === 'Transforms' || target === 'Editor'
              ? args.slice(1)
              : args
          trace.push({ target, method, args: recorded.map(toJson) })
          return result
        } finally {
          depth -= 1
        }
      }
    }
  }
}

/**
 * Slate offsets count UTF-16 code units; leptoslate counts Unicode scalar
 * values. Convert fixture serialization at the boundary.
 */
function textAtPath(children: unknown, path: unknown): string | undefined {
  if (!Array.isArray(path)) return undefined
  let node: any = { children }
  for (const index of path) {
    if (!node || !Array.isArray(node.children)) return undefined
    node = node.children[index]
  }
  return typeof node?.text === 'string' ? node.text : undefined
}

function convertPoint(point: any, children: unknown): any {
  if (!point || typeof point !== 'object') return point
  if (typeof point.offset !== 'number') return point
  const text = textAtPath(children, point.path)
  if (text === undefined) return point
  return {
    ...point,
    offset: Array.from(text.slice(0, point.offset)).length,
  }
}

function convertSnapshot(snapshot: any): any {
  const children = snapshot.children
  const selection = snapshot.selection
  if (!selection) return snapshot
  return {
    ...snapshot,
    selection: {
      ...selection,
      anchor: convertPoint(selection.anchor, children),
      focus: convertPoint(selection.focus, children),
    },
  }
}

function convertArguments(value: any, children: unknown): any {
  if (Array.isArray(value)) {
    return value.map(item => convertArguments(item, children))
  }
  if (value && typeof value === 'object') {
    if (Array.isArray(value.path) && typeof value.offset === 'number') {
      return convertPoint(value, children)
    }
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [
        key,
        convertArguments(item, children),
      ])
    )
  }
  return value
}

const EDITOR_INSTANCE_METHODS = [
  'insertText',
  'insertBreak',
  'insertSoftBreak',
  'insertFragment',
  'deleteBackward',
  'deleteForward',
  'deleteFragment',
  'setNodes',
  'splitNodes',
  'mergeNodes',
  'moveNodes',
  'insertNodes',
  'removeNodes',
  'wrapNodes',
  'unwrapNodes',
  'liftNodes',
  'select',
  'deselect',
  'collapse',
  'move',
  'setSelection',
]

/**
 * History fixtures drive the editor instance directly (`editor.insertBreak()`).
 * Wrap those methods just like the `Transforms` namespace.
 */
function instrumentEditorInstance(editor: any) {
  for (const method of EDITOR_INSTANCE_METHODS) {
    const original = editor[method]
    if (typeof original !== 'function') continue
    editor[method] = function (...args: unknown[]) {
      if (depth > 0) return original.apply(this, args)
      depth += 1
      try {
        const result = original.apply(this, args)
        trace.push({
          target: 'EditorInstance',
          method,
          args: args.map(toJson),
        })
        return result
      } finally {
        depth -= 1
      }
    }
  }
}

function fixtureFiles(category: Category) {
  const root = path.join(category.root, category.dir)
  const files: string[] = []

  const walk = (directory: string) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const full = path.join(directory, entry.name)
      if (entry.isDirectory()) walk(full)
      else if (
        entry.isFile() &&
        (entry.name.endsWith('.tsx') || entry.name.endsWith('.ts')) &&
        !entry.name.endsWith('.spec.ts') &&
        entry.name !== 'index.ts'
      ) {
        files.push(full)
      }
    }
  }

  walk(root)
  return files.sort()
}

function writeFixture(
  category: Category,
  source: string,
  data: unknown
) {
  const relative = outputRelative(category, source).replace(/\.tsx?$/, '.json')
  const target = path.join(OUT, relative)
  fs.mkdirSync(path.dirname(target), { recursive: true })
  fs.writeFileSync(target, `${JSON.stringify(data, null, 2)}\n`)
}

async function main() {
  if (!fs.existsSync(path.join(SLATE_TEST_ROOT, 'index.ts'))) {
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
    counts[category.name] = 0
    for (const sourceFile of fixtureFiles(category)) {
      const relative = outputRelative(category, sourceFile)
        .split(path.sep)
        .join('/')
      const upstreamSource = path
        .relative(SOURCE, sourceFile)
        .split(path.sep)
        .join('/')
      let module: any

      try {
        module = await import(
          `${pathToFileURL(rewriteFixture(sourceFile, shim, category)).href}?v=${imported++}`
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
        if (module.skip === true) {
          skipped.push({ source: relative, reason: 'marked skip upstream' })
          continue
        }

        if (category.name === 'operations') {
          const input = module.input
          const output = module.output
          assert.ok(input?.children, `${relative}: missing input editor`)
          assert.ok(output?.children, `${relative}: missing output editor`)
          assert.ok(
            Array.isArray(module.operations),
            `${relative}: missing operations`
          )

          writeFixture(category, sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: relative,
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
        } else if (category.name.startsWith('transforms/')) {
          assert.equal(
            typeof module.run,
            'function',
            `${relative}: missing run function`
          )
          assert.ok(module.input?.children, `${relative}: missing input editor`)
          assert.ok(module.output?.children, `${relative}: missing output editor`)

          // Capture the document before `run` mutates it.
          const inputSnapshot = convertSnapshot({
            children: toJson(module.input.children),
            selection: toJson(module.input.selection ?? null),
          })

          trace.length = 0
          callInputSnapshot = null
          module.run(module.input)
          // Prefer the document state observed at the first transform call.
          const replayInput = callInputSnapshot ?? inputSnapshot
          assert.deepEqual(
            {
              children: module.input.children,
              selection: module.input.selection ?? null,
            },
            {
              children: module.output.children,
              selection: module.output.selection ?? null,
            },
            `${relative}: fixture's own run/input/output disagree`
          )
          const transformCalls = trace.filter(
            call =>
              call.target === 'Transforms' ||
              (call.target === 'Editor' && call.method === 'addMark')
          )
          assert.ok(
            transformCalls.length > 0,
            `${relative}: no instrumented Transforms/Editor call`
          )

          writeFixture(category, sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: relative,
            },
            kind: 'transform',
            input: replayInput,
            calls: transformCalls.map(call => ({
              target: call.target,
              method: call.method,
              args: convertArguments(call.args, replayInput.children),
            })),
            output: convertSnapshot({
              children: toJson(module.output.children),
              selection: toJson(module.output.selection ?? null),
            }),
          })
        } else if (category.name === 'normalization') {
          assert.ok(module.input?.children, `${relative}: missing input editor`)
          assert.ok(module.output?.children, `${relative}: missing output editor`)

          writeFixture(category, sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: relative,
            },
            kind: 'normalization',
            input: convertSnapshot({
              children: toJson(module.input.children),
              selection: toJson(module.input.selection ?? null),
            }),
            output: convertSnapshot({
              children: toJson(module.output.children),
              selection: toJson(module.output.selection ?? null),
            }),
            ...(module.withFallbackElement
              ? { withFallbackElement: true }
              : {}),
          })
        } else if (category.name.startsWith('history/')) {
          assert.equal(
            typeof module.run,
            'function',
            `${relative}: missing run function`
          )
          assert.ok(module.input?.children, `${relative}: missing input editor`)
          assert.ok(module.output?.children, `${relative}: missing output editor`)

          const inputSnapshot = convertSnapshot({
            children: toJson(module.input.children),
            selection: toJson(module.input.selection ?? null),
          })

          trace.length = 0
          instrumentEditorInstance(module.input)
          module.run(module.input)
          const historyCalls = trace.filter(
            call =>
              call.target === 'EditorInstance' ||
              call.target === 'Transforms' ||
              call.target === 'Editor'
          )
          module.input.undo()
          assert.deepEqual(
            {
              children: module.input.children,
              selection: module.input.selection ?? null,
            },
            {
              children: module.output.children,
              selection: module.output.selection ?? null,
            },
            `${relative}: fixture's own run/undo/output disagree`
          )
          assert.ok(
            historyCalls.length > 0,
            `${relative}: no instrumented editor call`
          )

          writeFixture(category, sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: relative,
            },
            kind: 'history',
            input: inputSnapshot,
            calls: historyCalls.map(call => ({
              target: call.target,
              method: call.method,
              args: convertArguments(call.args, inputSnapshot.children),
            })),
            output: convertSnapshot({
              children: toJson(module.output.children),
              selection: toJson(module.output.selection ?? null),
            }),
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

          writeFixture(category, sourceFile, {
            upstream: {
              repository: 'ianstormtaylor/slate',
              commit: SHA,
              source: relative,
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
        counts[category.name] += 1
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
    console.log(`${category.name}: ${counts[category.name]}`)
  }
  console.log(`skipped: ${skipped.length}`)
  for (const entry of skipped) {
    console.log(`  - ${entry.source}: ${entry.reason.split('\n')[0]}`)
  }
}

await main()
