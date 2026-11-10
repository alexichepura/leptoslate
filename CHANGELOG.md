# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-11-10

### Changed

- Target Leptos 0.9 (`0.9.0-rc1`). `main` now follows the 0.9 line; Leptos 0.8
  users should stay on the `0.1.x` releases or the
  [`leptos-0.8`](https://github.com/alexichepura/leptoslate/tree/leptos-0.8) branch.
- Move the workspace to Rust edition 2024 and `resolver = "3"`. MSRV stays at
  1.88, which Leptos 0.9 requires.

### Added

- Replay the upstream Slate fixture suite in `leptoslate-core` tests, so the
  document model and transforms are checked against Slate's own
  `insertText`, `delete`, `splitNodes`, `wrapNodes` and related fixtures.

## [0.1.0] - 2026-10-20

### Added

- Initial release.
- `leptoslate-core`: Slate-compatible document model, paths, points, ranges,
  operations, transforms, normalization and history.
- `leptoslate`: the `Editable` Leptos component and `EditorHandle` for
  programmatic editing, marks, blocks, undo/redo and clipboard handling.
- `leptoslate-rich`: batteries-included preset with lists, tables, links,
  inline mentions, custom renderers and table/list transforms.
- Targets Leptos 0.8.
