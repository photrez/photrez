# 00 - Product Scope (MVP v1 Lock)

## Product Statement

Build a lightweight desktop image editor for Windows with familiar editing flow and a distinct product identity.

This file is intentionally narrow: it defines only the v1 release scope and excludes roadmap expansions.

## Target User

- Primary: content creator / UMKM.

## MVP Feature Set (Locked)

- Layer basic: add, delete, reorder, opacity.
- Selection + move + basic transform.
- Crop + resize image/canvas.
- Brush + eraser.
- Export JPG/PNG/WebP with quality settings.
- Pro-Suite Print Settings System (Paper preview viewport, scale/position/PPI inspector, paper size presets, unit converter, system printer enumeration, smart composite high-DPI print spooling; ICC color management deferred to post-v1).

## Non-Goals (MVP)

- Advanced retouching (spot healing, healing brush, clone stamp, red-eye).
- AI tools and cloud collaboration.
- Admin/CMS.
- PSD preview/editing workflow.
- ICC color profile conversion & soft-proofing engine (Color Management deferred to post-v1).
- Command palette and plugin API.
- Native project format: `.ptz` shipped in alpha.1 (ZIP container: `document.json` + per-layer PNG bitmaps). Future `.ptz` v2 with text/shape layer metadata is post-v1 (see roadmap).

## Performance Budgets (MVP)

- Installer size: `< 80 MB`
- Idle RAM: `< 250 MB`
- Startup time: `< 2s` (target device: 4GB RAM + SSD)

## Brand Direction

- Familiar workflow with Photrez-owned identity.
- New visual language (icon set, naming, spacing, palette, typography).

## Exit Criteria for Scope Lock

- Every MVP feature has acceptance criteria in PRD.
- Every non-goal is explicitly listed.
- Performance budgets are measurable in test plan.
