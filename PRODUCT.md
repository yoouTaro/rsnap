# Product

## Platform

web (the landing site in `site/`; the product itself is a native macOS app built with Rust and egui)

## Users

People who paste screenshots all day: into AI chats, issues, pull requests, docs and team chat. Same audience as rjot, AI-heavy builders, plus anyone who explains things with pictures.

## Product Purpose

rsnap turns a capture into a ready-to-paste image in seconds: ⌥⇧S, drag, frame it on a backdrop, point at things, hide secrets, ↩. Success: someone captures, annotates and pastes their first shot within a minute of installing.

## Positioning

Small, free and open source (MIT), local-only, one native binary. Needs only Screen Recording, not Accessibility. The editor shows the real render, the same pixels it copies. Khmer text annotations are shaped correctly. Never name competitors.

## Capabilities and Constraints

- Capture: `screencapture -i` (region, or Space for a window without its shadow).
- Editor: arrow, box, highlight (multiply), redact (pixelate), numbered steps, text, crop; 6 colors; undo/redo.
- Frame: backdrops (None, Dusk, Peach, Ocean, Lime, Night gradients; Paper, Graphite, White), padding, corners, shadow.
- Output: PNG to the clipboard (↩/⌘C), or saved (⌘S).
- Updates only on request (GitHub, SHA-256 checked). Settings in `~/Library/Application Support/dev.rsnap.rsnap/settings.json`.
- macOS 11+, Apple silicon. Not notarized. Release v0.0.1-alpha, zip about 5.8 MB.

## Brand Commitments

- Name: rsnap, lowercase. Icon: `assets/logo.svg` (viewfinder corners around a violet framed shot, a red arrow). Accent violet (#9B8CFF dark, #6A55E8 light); rgrip is blue, rjot copper.
- Voice: modest and plain, like rgrip and rjot. No emoji. Author: Taro (GitHub yoouTaro).

## Evidence on Hand

Images rendered by rsnap's own compositor (`crates/core/examples/demo.rs`) in `docs/img/`; app screenshots from `scripts/screenshots.sh` in `docs/screenshots/`. No users, testimonials or benchmarks; don't invent any.
