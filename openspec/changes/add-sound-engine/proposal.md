# Proposal

## Why

After `add-simulation-core` the game runs and emits sound commands every frame, but nothing turns them into sound. Digger's audio is a large part of its character: twelve PC-speaker effects, a level-done jingle and four tunes, all defined by exact divisor sequences on a 72.8 Hz tick. This change specifies that behaviour once and builds it as a pure, testable library before any audio device exists, so that `add-desktop-app` only has to pull samples.

## What Changes

- Add the crate `digger-sound`: sound commands in, mono 16-bit PCM samples out. No device, no clock, no I/O, no `unsafe`.
- Implement the twelve effects (fall, wobble, break, fire, explode, bonus siren, emerald click, emerald scale, gold, eat monster, death, extra life) and the level-done jingle as explicit state machines advanced once per sound tick, with upstream's effect priority order and effect-over-music rule.
- Implement the four tunes (normal, bonus, death dirge, last-life battle tune) with upstream's note encoding, per-tune tempo and attack/decay/release envelope.
- Synthesise a 50 % square wave at `1193181 / divisor` Hz (integer division), with an optional 1 kHz high-pass and 4 kHz low-pass filter, a working master volume, sound and music toggles and pause.
- Fix the command boundary: the exact list of sound commands the simulation emits, the control inputs the frontend sends, and the informational events the engine reports back.
- Join game thread and audio thread with a lock-free single-producer queue; provide a manual driver (tests render synchronously) and a null driver (no device: commands are accepted and discarded).
- Extend `xtask import-assets` to import the four music tables as a data blob, guarded by a hash test.
- Add `xtask render-sound`, which renders a scripted command list to a WAV file for the one-off review by ear of the golden renders.
- Record the sound quirks kept and fixed in `docs/compat-quirks.md`.

Decisions recorded by this change (deliberate differences from upstream):

- Fidelity is **audible**, not sample-identical to the C build: same notes, durations, effect shapes and priorities, verified at tick level.
- The level-done jingle always plays; upstream's "silent until some music has played" quirk is fixed.
- The death effect keeps upstream's audible shape: divisors at or above 0x4000 are silent, so it is heard on ticks 4 to 22 of 51.
- Master volume works (upstream reads the setting and ignores it).
- Sound can be turned on after starting quiet; "sound off" is a mute, not a frozen engine.
- Sound never influences the simulation. Upstream's two wall-clock waits (level-done jingle, dirge hold) are fixed frame counts owned by the simulation.
- The data races of the original are removed by design: all audio state is owned by one value on the audio thread and reached only through queues.

Out of scope:

- Opening the audio device, device errors, the buffer-size setting, sample-format and channel conversion, feeding the engine from the device callback: `add-desktop-app`.
- Key bindings for the sound and music toggles, command-line and configuration-file settings for sound, music, volume, sample rate and filter: `add-desktop-app` (this change only defines the engine inputs they map to).
- Emitting the sound commands from game events and the fixed frame counts that replace the two waits: `add-simulation-core` (already applied; this change checks the command names against its interface).
- Browser audio: `add-web-build-and-releases`.
- A sample-exact comparison against the C build and any C reference harness: not planned (no C in this project).

## Capabilities

### New Capabilities

- `sound`: every sound effect and tune (triggers, tick-level pitch sequences, durations, priorities), the sound tick, tone synthesis and output format, filtering, toggles, pause, master volume, silent operation without a device, the sound command boundary, and the guarantee that sound never feeds back into the simulation.

### Modified Capabilities

None.

## Impact

- New crate `crates/digger-sound` (library), added to the workspace. Depends on `digger-core` only for the sound command type, and on `rtrb` for the lock-free queue. Dev-dependencies: `proptest`, `sha2`.
- `xtask`: `import-assets` gains the music tables; new `render-sound` subcommand.
- `assets/`: new `music.bin` and its index entry; golden hash file under `crates/digger-sound/tests/golden/`.
- `docs/compat-quirks.md`: new "Sound" section.
- `digger-core`: no behaviour change. If its sound command enum differs from the list in this change's design, it is aligned in the first task group (names only).
- Consumed later by `add-desktop-app` (audio device) and `add-web-build-and-releases`.
