# Product specification

## Vision

Nulis is a professional, offline-capable, cross-platform editor for text, code, structured logs, hexadecimal data, and files larger than available memory. It uses original branding and UX, a headless Rust core, and versioned extension surfaces.

## Product principles

Priority order is data integrity, architectural correctness, bounded-memory large-file operation, correctness, performance, security, extensibility, platform integration, and UX polish.

## Users and workflows

- Developers editing source trees, comparing files, using terminals, and automating repeatable work.
- Operators inspecting multi-gigabyte logs with streaming search.
- Data and firmware users editing binary files and fixed-width records.
- Power users using multiple selections, rectangular operations, themes, commands, and plugins.

## Non-negotiable requirements

- Source-file memory use must not grow linearly with file size.
- The first viewport may render before background indexing finishes; indexing is cancellable.
- All file offsets are 64-bit safe.
- Saving modified large files uses temporary output, flush/validation, and atomic replacement where supported.
- React owns presentation state, not the authoritative document.
- Commands are the shared entry point for menus, keys, macros, and plugins.
- Plugins use a versioned, capability-checked API and cannot access private core structures.
- Untrusted previews and scripts receive no ambient native authority.

Detailed delivery is staged in `ROADMAP.md`; implementation truth is tracked in `FEATURE_PARITY.md`.

