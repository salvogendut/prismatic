# Prismatic development

- Implement the frontend in Rust, using SDL3 for host services. Use Cargo.
- The emulation engine is our sibling `../iris` fork. Keep CPU/device changes
  there and integration code here. Inspect its current APIs before adapting
  them; read its `AGENTS.md` before editing the fork.
- The default fetch-and-build workflow is `make`: use
  `https://github.com/salvogendut/iris.git`, branch `xiri`. Keep `IRIS_REPO` and
  `IRIS_BRANCH` editable in the Makefile and overridable on the command line.
  The helper owns isolated checkouts under `target/bootstrap`; never reset
  local work or switch a developer's `../iris` checkout to satisfy a build.
  Direct Cargo builds still use `../iris`; check its branch before working.
  Default bootstrap features are `pcap,idle-pause,jitv2`. Add `perfetto` only
  on cores where it compiles; avoid `developer` for performance builds because
  its instrumentation disables the GPR cache. Development happens only on
  `xiri`; `perf/sgiemu-port` is merged and retired. Retain `main` compatibility:
  detect optional embedding APIs from the checked-out sources in `build.rs`,
  never from the branch name. The integrated SGIEmu optimizations are opt-in,
  off by default, and must remain configurable per machine after rebuilding.
  Grey out unavailable controls, preserve saved preferences, and test `main` compatibility
  in an isolated checkout as well as the active `xiri` core. Do not switch the active Iris checkout merely
  for compatibility tests or use the former `xiri/netbsd-newport` branch. Preserve NetBSD and other
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
  [docs/testing.md](docs/testing.md#checks). Its environment override keeps Iris's
  shader corpus in `/tmp`.
  Avoid `cargo fmt --all`: it follows path dependencies into the Iris checkout.
