# Vynull Touch Roadmap

Updated November 13, 2026

Vynull Touch is a touchscreen companion for Vynull: browse the library, prepare
tracks, and monitor and load the decks. It is built for 800×480 first; larger
displays get more player information. The milestones set an order of work, not
dates.

## Where things stand

Everything below is built. The latest build passed 283 tests (the opt-in
hardware-audio test skipped) and Clippy. What's missing across the board is
validation on the physical touchscreen and with live CDJs; see Next steps.

**Library**

- Collection and playlist tables with artwork and waveform previews, search with
  a touch keyboard, filters, and ascending, descending or original sorting on
  any column.
- Live Key/BPM highlighting against the master deck, a chosen player or none,
  using saved key-match rules (relative A/B, +1 and −1).
- P1–P4 badges on tracks loaded on players, and Preview/Pause from the lists.
- Prepare: a shortlist kept in a shared Vynull playlist, with quick adds,
  reordering, removal and loading.
- Match a player: tracks that fit a deck's pitched tempo and key.
- Rating, color and tag edits; playlist, folder, tag and category management.

**Inspect**

- Overview and detailed waveforms with the beat grid, phrase strips, remembered
  zoom and smooth curves at high zoom.
- Local audio preview, beat jumps, a millisecond cursor readout and Quantize.
- Hot cues A–H in rekordbox's 16 cue colors with per-pad defaults, cue editing
  and saved loops.
- Beat-grid editing (nudges, beat 1, a BPM keypad) with an Analysis lock, and
  Undo/Redo.

**Players**

- Cards, Waveforms and a focused view, two-player comparison, custom pairs, and
  four cards when the display has room.
- Smooth estimated playheads, held CUE included; phrase strips with the current
  section, and bars/beats countdowns to the next phrase and the next cue.
- Loading with session and source checks, and confirmation before replacing a
  playing track.

**Settings and diagnostics**

- Saved server, startup page and waveform style; preview audio output and volume.
- CDJ & mixer: Vynull's shared hardware profile (46 fields) and the CDJ menu's
  categories, sort menu and track-detail column.
- Diagnostics: status, versions, request errors, logs and a copyable report.

**Record**

- Session recording of the device's stereo input to WAV: 16 or 24 bits, Auto or
  44.1–192 kHz, level meters, a REC timer that stays in the status bar, a saved
  input, format and folder, and numbered parts near 1 GiB.

**Releases:** `scripts/release.py` builds natively, checks checksums, installs
atomically, keeps the previous build for rollback and can restart the open UI.

## Next steps

1. Walk through the features on the physical touchscreen: navigation
   and Back, Prepare with two clients, the editors, the settings pages and grid
   editing with Undo/Redo.
2. Check them against live CDJs: playhead smoothness, countdowns, badges, match
   highlighting, the CDJ menu after browser changes, and hardware settings
   writes.
3. Measure performance on the Pi and set budgets from it.
4. Add recently played history, and document the desktop shortcut and
   autostart setup.
5. Test recording from a HiFiBerry, then add playback/import and track-change
   markers to recordings.

## Milestones

| Priority | Milestone | Status |
| --- | --- | --- |
| Now | 1 Everyday use | Built; touchscreen walkthrough pending |
| Now | 2 Repeatable releases | Done except the shortcut and autostart docs |
| Next | 3 Player monitoring and diagnostics | Layouts done; timing states, log search and performance pending |
| Next | 4 Finding the next track | Done except history and browsing-state polish |
| Later | 5 Track preparation | Editors done; readback and refresh polish pending |

Items are checked when built. A milestone is done when its
criteria are verified on the hardware.

### 1 Everyday use

- [x] Back from Inspect returns where you came from, keeping the player pair,
  playlist, search, selection and scroll position.
- [x] Saved server address and startup page: command-line options, then saved
  preferences, then defaults.
- [x] Connection recovery with progress, backoff and a manual retry, kept apart
  from loads and edits.
- [x] Library and cue changes refresh in the background without losing context.
- [x] A replaced library is a new session: old selections and drafts are cleared.

**Done when** a touchscreen walkthrough covers Players → Inspect → Back, an edit
from another client, a daemon restart, a network loss and a library replacement,
with no wrong-track selection, lost context, unexpected audio or replayed writes.

### 2 Repeatable releases

- [x] The project and app are named Vynull Touch (`vynull-touch`).
- [x] Release tooling in the repository: preflight checks, staging, checksums,
  atomic install, rollback and an opt-in restart of the open UI.
- [x] UI and daemon versions, and daemon capabilities, shown in Diagnostics.
- [x] ARM64 release artifacts with dependency information and checksums. Native
  builds stay the release path; a GCC cross build is future work.
- [ ] Document the desktop shortcut and optional autostart, with configuration
  paths and how to recover from a failed update.

**Done when** a clean checkout builds, tests, installs, launches and rolls back
with documented commands, and a missing drive or failed build leaves the
installed UI usable.

### 3 Player monitoring and diagnostics

- [x] Waveforms layout with stacked scrolling waveforms, and saved waveform
  styles (Auto, RGB, 3-band, Blue).
- [ ] Readable timing state: source, estimated position, stale status, missing
  analysis and disconnected players. CUED and paused waveforms are done.
- [ ] Diagnostics log search and analysis availability. Status and the report
  are done.
- [ ] Measure frame pacing, CPU, memory and request latency on the Pi. Target
  60 fps for two moving waveforms on a Pi 5.
- [ ] Tune redraws and asset work from those measurements, and exercise pitch
  changes, loops, seeks, track swaps, delayed status and reconnects on hardware.

**Done when** a two-hour Pi session runs without crashes, memory growth or lost
touch responsiveness, the measurements are recorded, and live CDJs confirm the
timing, with four physical players tested apart from simulated ones.

### 4 Finding the next track

- [x] Sorting and filters (BPM, key, genre, rating, color, tags), remembered
  across restarts.
- [x] Library table with waveform previews beside the artwork.
- [x] Prepare shortlist.
- [x] Match a player, live Key/BPM highlighting and key-match rules.
- [ ] Recently played history, linking to Inspect for tracks still in the
  library.
- [ ] Keep browsing state between playlists and Players, and show clear empty,
  missing-file and unavailable-track states.

**Done when** a track can be found, shortlisted, inspected and loaded with the
800×480 touchscreen alone, and a replaced library can't point history or the
shortlist at the wrong track.

### 5 Track preparation

- [x] Rating, color and tag edits in Inspect.
- [x] Tag and category management.
- [x] Playlist and folder management.
- [x] Beat-grid editing with an Analysis lock and Undo/Redo.
- [ ] Read every edit back and refresh related views; keep drafts on recoverable
  errors and make uncertain saves explicit.

**Done when** edits survive reopening the track and restarting the daemon,
including locked grids, concurrent edits, changed source files, lost replies and
persistence failures, and no edit or preview ever moves a deck.

## Depends on Vynull

- Tracks and cues have revision endpoints; playlists don't, so the app polls the
  playlist tree and the open playlist.
- Prepare is a shared playlist. Vynull has no atomic playlist compare-and-swap,
  so a change between the app's check and its write can't be ruled out.
- Grid Undo/Redo uses `POST /api/analysis/beatgrid/restore` (Vynull #183).
  Vynull still has no atomic multi-step grid writes or persistence receipts.
- Browser and hardware settings write through `/api/menu-items` and
  `/api/settings`, which have no revision check: the last write wins.
- Recently played history needs Vynull's history identity contract checked.

Missing guarantees like these are Vynull work; a check in the app doesn't make a
write atomic.

## Scope

The first stable release covers browsing, monitoring, deliberate loading and
light preparation. Later candidates: bulk import and file management, USB
export, smart-playlist authoring and full metadata administration. Controlling
the CDJs (play/pause, seeking, hot cues) needs a separate decision. Portrait
layouts, displays below 800×480 and running without a desktop compositor are out
of scope for now.

## Release checks

For each release, run the tests, formatting and lint checks, and a native ARM64
build. Try changed interactions on the physical touchscreen, since VNC doesn't
cover touch, at 800×480 and a larger size. Run audio or live-player checks when
those paths change, and keep the release evidence with the build ID.
