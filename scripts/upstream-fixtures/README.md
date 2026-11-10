# Upstream fixture extractor

Converts Slate's TypeScript fixture modules into JSON replayed by
`crates/leptoslate-core/tests/upstream.rs`.

The extractor is a development tool, not part of the Rust build. Generated
fixtures are committed, so `cargo test` works offline without Node.

## Usage

```bash
npm ci
npm run extract
```

Pinned upstream commit:
`fd42a8046db8da329f4584dcf2a68ac7abd3137b`.

Environment overrides:

- `UPSTREAM_SHA` — extract a different Slate commit.
- `UPSTREAM_CACHE` — where the tarball and work directory live
  (default: `~/.cache/leptoslate-upstream`).

## How it works

1. Downloads the pinned Slate tarball into the cache and extracts it.
2. Creates generated ESM packages (`.deps/`, `node_modules/`) that re-export
   Slate's TypeScript sources (`slate`, `slate-hyperscript`, `slate-history`),
   so no upstream build step is needed.
3. Rewrites fixture imports that point at Slate's Mocha harness to a local JSX
   shim.
4. Instruments `Path`, `Point`, `Range`, `Transforms`, `Editor` and editor
   instances to capture the arguments of each top-level call.
5. Evaluates each `.ts`/`.tsx` fixture with `tsx`, checks the fixture's own
   `input`/`run`/`output` agree, and writes JSON under
   `crates/leptoslate-core/tests/upstream/fixtures`.
6. Writes `skipped.json` with reasons for any fixture that could not be
   evaluated.

Slate suites are read from `packages/slate/test` and, for history fixtures,
`packages/slate-history/test`.

`node_modules/` and `.deps/` are generated and git-ignored.

## Output shape

`interfaces` fixture:

```json
{
  "upstream": { "repository": "...", "commit": "...", "source": "..." },
  "kind": "interface",
  "input": {},
  "calls": [{ "target": "Path", "method": "transform", "args": [] }],
  "output": []
}
```

`operations` fixture:

```json
{
  "upstream": { "repository": "...", "commit": "...", "source": "..." },
  "kind": "operations",
  "input": { "children": [], "selection": null },
  "operations": [],
  "output": { "children": [], "selection": null }
}
```

`transforms` fixture:

```json
{
  "upstream": { "repository": "...", "commit": "...", "source": "..." },
  "kind": "transform",
  "input": { "children": [], "selection": null },
  "calls": [{ "target": "Transforms", "method": "delete", "args": [] }],
  "output": { "children": [], "selection": null }
}
```

`normalization` and `history` fixtures use the same input/output snapshot
shape. Normalization fixtures have no `calls`; history fixtures additionally
run `undo()` after replaying their calls.

Selection offsets are converted from UTF-16 code units to Unicode scalar
values when fixtures are written. Input snapshots are captured before the
upstream `run` mutates the editor.
