# Prismatic development

- Implement the frontend in Rust, using SDL3 for host services. Use Cargo.
- The emulation engine is our sibling `../iris` fork. Keep CPU/device changes
  there and integration code here. Inspect its current APIs before adapting
  them; read its `AGENTS.md` before editing the fork.
- Prismatic `main` uses the user-requested `perf/sgiemu-port` branch of
  `../iris` (based on `xiri`) after integrating the performance build. Keep this
  Iris branch until its performance changes are integrated into `xiri`.
  Build with `pcap,idle-pause,perfetto,jitv2`, without `developer`: developer
  instrumentation disables the new GPR cache. Check the Iris branch before
  working; do not substitute upstream
  `main` or the former `xiri/netbsd-newport` branch. Preserve NetBSD and other
  guest OS compatibility when integrating changes; do not assume IRIX-only use.
- Match the native Happy Years UI conventions in `../1984`, `../1985`,
  `../1986`, and especially `../1989`: emulator-first window, F9 overlay,
  keyboard navigation, status hints, notifications and Tinker-gated controls.
- Keep settings drafts separate from live configuration. Save must validate
  and persist successfully before closing. Discard must have no runtime
  side effects. Confirm changes that restart a running machine.
- Keep SDL window/render/event work on the main thread. Machine work must
  not block the UI. Do not use unbounded queues for video or audio.
- Do not claim preview controls implement emulation. Keep README status and
  `docs/architecture.md` accurate as integration progresses.
- Run `cargo fmt -p prismatic --check`,
  `cargo clippy --locked --all-targets --no-deps -- -D warnings`,
  `cargo test --locked` and the dummy-video smoke test for frontend changes.
  Add focused tests for behavior and failure paths, not drawing constants.
- For lifecycle or display changes, also run the diskless PROM smoke test in
  README.md. Its environment override keeps Iris's shader corpus in `/tmp`.
  Avoid `cargo fmt --all`: it follows path dependencies into the Iris checkout.
