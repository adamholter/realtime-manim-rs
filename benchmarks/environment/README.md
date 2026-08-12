# Environment manifests

`current.json` is a sanitized snapshot from `realtime-manim-env-capture`.

Refresh deliberately when benchmark hardware, display mode, OS/toolchain, or load profile changes:

```sh
REALTIME_MANIM_LOAD_PROFILE=chrome-slack-codex \
  cargo run -p realtime-manim-env-capture
```

Review stdout, then update `current.json`. Never add raw `system_profiler`, hostnames, usernames, paths, serial numbers, UUIDs, account identifiers, or process lists.
