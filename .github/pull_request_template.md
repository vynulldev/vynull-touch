<!-- Thanks for contributing to Vynull Touch! Keep PRs focused; explain the "why", not just the "what". -->

## What & why

<!-- What does this change and why? Link any related issue, e.g. Fixes #123 -->

## Testing

<!-- The tests and captures can't feel a touchscreen, and loads and edits only really happen
     against a running Vynull. Say what you tried and where. -->

- **Display:** <!-- e.g. Pi 5 + 800×480 touchscreen, or "desktop window at 800×480 and 1280×800" -->
- **Vynull:** <!-- server version you tested against, or "demo only" -->

## Screenshots

<!-- For visible changes, before and after. `python3 scripts/capture.py --help` lists a capture
     for each feature. Use the demo or fictional data, never your own library. -->

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked` pass
- [ ] `python3 -m unittest discover -s scripts -p test_release.py` passes, **or** this doesn't touch `scripts/`
- [ ] Layouts still fit at 800×480, 1024×600 and 1280×800
- [ ] New source files carry an SPDX header (`GPL-3.0-or-later`)
- [ ] Loads and library edits were checked against a running Vynull, **or** this change touches neither
- [ ] I agree my contribution is licensed under the project's [GPLv3](../LICENSE)
