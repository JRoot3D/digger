# Desktop test checklist

The manual checks of `add-desktop-app` (design D15), run once per platform
before the change is archived. Everything else is covered by the automated
tests (`cargo test --workspace`), which need no display or sound device.

How to fill in a result cell: `pass`, `fail: <issue or note>`, or `n/a:
<reason>`, followed by the date and the machine (for example `pass
2026-11-12, Fedora 43 / GNOME`). A cell left as `—` has not been run. A
check that needed a fix is re-run after the fix and the cell records the
run that passed. An item counts as done on a platform only when a person
has watched it (or, where a row says so, a script has checked it on a
real display server of that platform).

Use a fresh data and configuration location for the file checks, and keep
it for the whole run:

```
digger --config /tmp/dt/config.toml --data-dir /tmp/dt/data   # explicit files
digger                                                         # platform folders (item F1)
```

The frame counter of item P1 is switched on with the environment variable
`DIGGER_FRAME_COUNTER=1`; it prints `frame counter: <n> frames in <t> s` on
standard error every 10 seconds.

## Window and picture

| # | Check | Expected | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|---|---|
| W1 | Window title | Start `digger`. The title bar reads `D I G G E R`. | — | — | — | — |
| W2 | Icon | The window (title bar, task bar or dock, Alt+Tab switcher) shows the Digger icon where the platform has per-window icons. Wayland and macOS have none: note `n/a` if nothing is shown. | — | — | — | — |
| W3 | Default size | The picture area is 1280×800 physical pixels, picture at scale 2 with no black border. | — | — | — | — |
| W4 | Resize and letterboxing, `integer` | Resize to about 1920×1080: picture 1280×800 centred, black bars. Resize to 1300×1300: picture 1280×800 at offset (10, 250). Try to shrink below 640×400: the window stops at 640×400. No smoothing: pixels are sharp squares at every size. | — | — | — | — |
| W5 | Scale modes `fit` and `stretch` | `--scale fit` at about 1920×1080: 1728×1080 with 96 black columns each side. `--scale stretch`: the picture fills the window. | — | — | — | — |
| W6 | Alt+Enter and F11 | On the title screen, in a game and during a pause: Alt+Enter and Alt+keypad Enter switch to borderless fullscreen on the current monitor and back; F11 does the same. The title screen keeps running and no game starts; a pause stays paused. Back in the window, the previous size and position return. | — | — | — | — |
| W7 | Pointer hidden in fullscreen | In fullscreen the mouse pointer is not visible over the picture; back in the window it is visible again. | — | — | — | — |
| W8 | Start fullscreen | `digger --fullscreen`: the first picture shown is fullscreen. In a normal start, pressing F11 once and quitting writes `fullscreen = true` under `[video]` of the configuration file. | — | — | — | — |
| W9 | HiDPI display | On a display with scale factor 2 (or 1.5): the window opens at 1280×800 physical pixels (640×400 logical at factor 2), the picture is sharp, and moving the window between monitors of different factors keeps it correctly scaled and letterboxed. | — | — | — | — |

## Pacing

| # | Check | Expected | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|---|---|
| P1 | Pacing by stopwatch | `DIGGER_FRAME_COUNTER=1 digger`, start a game and time one minute with a stopwatch from the first counter line: the count rises by 750 (±1) per 60 s of wall-clock time (7500 ± 1 per 600 s). | — | — | — | — |
| P2 | Speed keys | In a game, `=` seven times makes the game visibly faster (frame time 10 ms); `-` slows it down again. With `--speed 20` the counter shows 25 frames per second. | — | — | — | — |
| P3 | Stall recovery | Drag the window around for a few seconds during a game (or suspend the machine briefly): the game does not run a burst of fast frames afterwards. | — | — | — | — |

## Sound

| # | Check | Expected | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|---|---|
| S1 | Default device | The title screen is silent; starting a game plays the main tune; effects (emerald, fire) are heard. On a stereo device both channels carry the sound. | — | — | — | — |
| S2 | No device | With every output device disabled: one line `digger: notice: no audio device …` on standard error, the game is fully playable in silence. Enable a device, press F9 twice: sound is heard after the press that switches sound on. | — | — | — | — |
| S3 | Device removed while playing | Unplug the headphones or USB audio device in use during a game: one notice, the game goes on silently; plug it back and press F9 twice: sound returns. | — | — | — | — |
| S4 | F9 after `--quiet` | `digger --quiet`, start a game: silent. F9: effects and music are heard from then on. F7 stops only the tune; effects continue. | — | — | — | — |

## Gamepads

| # | Check | Expected | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|---|---|
| G1 | One gamepad | Start on the title screen starts a game; d-pad and left stick move Digger; South and West fire; Start pauses, any button resumes; Select changes mode on the title screen. | — | — | — | — |
| G2 | Two gamepads | In two players simultaneous mode the first pad drives the first Digger and the second pad the second. | — | — | — | — |
| G3 | Hot-plug | Connect a pad while the title screen is shown: its Start starts a game without a restart. Unplug player 1's pad while holding the d-pad right during a game: the game shows `PRESS ANY KEY`; after resuming, Digger does not move right by itself. Reconnect: the pad is player 1 again. | — | — | — | — |
| G4 | Stick drift | With the stick at rest (or slightly off-centre, below half deflection) Digger does not move. | — | — | — | — |

## Closing and files

| # | Check | Expected | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|---|---|
| C1 | Close button in each scene | Close the window (title bar button, or the platform's close shortcut) on the title screen, during a game, during a pause, during a playback (`--play`), during initials entry (after one letter) and during key redefinition. Each time the program ends at once with exit status 0; after the initials case the score file is unchanged and readable; after the redefinition case the keys are as before. | — | — | — | — |
| C2 | Close during a recorded game | `digger --record /tmp/dt/close.drf`, close the window during play: the file exists and `digger --verify /tmp/dt/close.drf --headless` prints its result line and exits with 0. | — | — | — | — |
| F1 | Files in the documented folders | Without `--config` and `--data-dir`: press F7 on the title screen and quit; `config.toml` appears in the configuration folder of [the manual's table](../manual.md#where-files-are-kept) with `music = false`. Enter a high score: `scores.toml` appears in the data folder. Press F8 on the title screen: the recording appears in its `recordings` folder. Nothing is written elsewhere (no file in the current directory or the home directory). | — | — | — | — |

## Recordings

| # | Check | Expected | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|---|---|
| R1 | `--verify` status 3 by pressing F10 | `digger --verify tests/recordings/data/sobodig.drf` (in the window): press F10 halfway. The program ends with exit status 3 and prints no result line. | — | — | — | — |
| R2 | Taking over a playback | `digger --play tests/recordings/data/sobodig.drf --take-over-at 200`: until tick 200 (16 s at the default speed) the recording drives Digger and the arrow keys do nothing; after it, the arrow keys and F1 drive Digger. At the game's end no initials are asked for. | — | — | — | — |

## Results

| Platform | Run by, date, machine | Open failures |
|---|---|---|
| Linux X11 | — | — |
| Linux Wayland | — | — |
| Windows | — | — |
| macOS | — | — |
