# Digger — user manual

This is the manual of `digger`, a re-creation in Rust of Digger Remastered,
the 1983 Windmill Software game as reverse-engineered by Andrew Jenner and
extended by Maksym Sobolyev. It runs on Linux (X11 and Wayland), Windows and
macOS, and in a web browser (see [Playing in a web
browser](#playing-in-a-web-browser)).

Two tables in this file are checked against the program by its tests: the
option list (`manual_option_list_matches_cli`) and the settings table
(`manual_settings_table_matches_schema`). Edit them together with the code.

## Contents

1. [System requirements](#system-requirements)
2. [Starting the game](#starting-the-game)
3. [Playing](#playing)
4. [Game modes](#game-modes)
5. [Keys](#keys)
6. [Gamepads](#gamepads)
7. [Redefining the keys](#redefining-the-keys)
8. [High scores](#high-scores)
9. [Command line](#command-line)
10. [Exit statuses](#exit-statuses)
11. [Settings file](#settings-file)
12. [Where files are kept](#where-files-are-kept)
13. [Recordings](#recordings)
14. [Level files](#level-files)
15. [Playing in a web browser](#playing-in-a-web-browser)
16. [Differences from upstream Digger](#differences-from-upstream-digger)

## System requirements

The release archives on the project's releases page each hold the program,
this manual (`MANUAL.md`), the licence (`LICENSE`), the credits (`NOTICE`)
and the licences of the libraries built into the program
(`THIRD-PARTY-LICENSES.html`), in one folder named after the archive.

| Archive | Runs on |
|---|---|
| `digger-<version>-linux-x86_64.tar.gz` | 64-bit PC Linux with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 or later), an X11 or Wayland desktop and, for sound, ALSA or a sound server with an ALSA plug-in (PulseAudio, PipeWire) |
| `digger-<version>-linux-aarch64.tar.gz` | The same on 64-bit ARM Linux |
| `digger-<version>-windows-x86_64.zip` | 64-bit Windows 10 or 11; nothing else needs installing |
| `digger-<version>-macos-universal.tar.gz` | macOS 11 Big Sur or newer, on Apple silicon or Intel |
| `digger-<version>-web.tar.gz` | Any static web server (see [Playing in a web browser](#playing-in-a-web-browser)) |

Any graphics hardware that can show a desktop is enough: the picture is
drawn by the processor. Older Linux systems can build the program from
source with a Rust toolchain (see `README.md`).

The downloads are not signed, so each system asks once before the first
start:

- **Linux:** unpack the archive (`tar xzf digger-<version>-linux-x86_64.tar.gz`)
  and run `./digger` in the folder it makes. To get a menu entry, copy
  `digger` to `~/.local/bin/`, `digger.png` to
  `~/.local/share/icons/hicolor/64x64/apps/` and `digger.desktop` to
  `~/.local/share/applications/`.
- **Windows:** unpack the zip file with *Extract All*, then double-click
  `digger.exe`. When SmartScreen says *Windows protected your PC*, choose
  *More info* and then *Run anyway*. The program also opens a console
  window, which shows its notices; `digger.exe --help` in a terminal lists
  the options.
- **macOS:** unpack the archive (double-click it in Finder) and move
  `Digger.app` to *Applications*. The first time, Control-click (or
  right-click) `Digger.app`, choose *Open* and confirm with *Open*; on
  macOS 15 and later, if no *Open* button is offered, open *System
  Settings → Privacy & Security* and choose *Open Anyway* next to the
  message about Digger. The command line is reached as
  `Digger.app/Contents/MacOS/digger`.

## Starting the game

Run `digger` with no arguments. A window titled `D I G G E R` opens with a
1280×800 picture area showing the title screen: the high-score table of the
selected mode on the left, the cast of characters being introduced on the
right, and the selected mode under the title.

On the title screen:

- **Any ordinary key**, or a gamepad's fire or Start button, starts a game in
  the selected mode. The keys that do something else on the title screen
  (mode change, redefine, quit, save recording, sound, music, speed and
  fullscreen) and the modifier keys (Shift, Control, Alt, the logo keys) do
  not start a game. A key still held from before the title screen appeared
  does not start one either.
- **N** (or the gamepad's Select button) selects the next game mode.
- **K** opens the key redefinition screen.
- **F8** saves the recording of the last game played (see
  [Recordings](#recordings)).
- **F10** ends the program.

The window can be resized down to 640×400. The picture is shown at the
largest whole-number scale that fits, centred on black; the `--scale` option
and the `video.scale` setting choose other ways of filling the window.
**Alt+Enter** or **F11** switches between the window and borderless
fullscreen at any time; the mouse pointer is hidden in fullscreen.

When the program cannot open a display it prints one line starting with
`digger: error:` and exits with status 69. When there is no sound device it
prints `digger: notice: no audio device …` once and runs silently; pressing
**F9** (the sound toggle) tries to open the device again.

## Playing

You steer Digger through the earth of each level. Tunnels stay open behind
him.

| Event | Points |
|---|---|
| Eating an emerald | 25 |
| Eight emeralds in a row, each soon after the previous one | 250 more |
| Collecting gold from a broken bag | 500 |
| Killing a monster with a fireball or a falling bag | 250 |
| Eating the bonus (cherry) | 1000 |
| Eating monsters in bonus mode | 200, 400, 800, … for each in turn |

- **Gold bags** wobble and fall when the earth below them is dug away. A bag
  that falls further than one row breaks into gold; a falling bag kills
  Digger or monsters below it. Digger can push a bag sideways.
- **Monsters** come in two kinds. Nobbins follow the tunnels; a nobbin may
  turn into a hobbin, which digs through the earth.
- **Fire** shoots a fireball in the direction Digger faces. It needs time to
  recharge before the next shot.
- When enough monsters have appeared, the **bonus** cherry shows up. Eating
  it starts bonus mode: the picture changes colour and for a short time
  Digger can eat the monsters.
- A level is complete when every emerald is eaten or every monster is dead.
  There are eight level layouts; after level 8 the last four repeat, and
  the game gets harder up to level 10.
- You start with three lives. A **bonus life** is given every 20000 points
  (a level file can choose another step), up to five lives, unless
  `--unlimited-lives` lifts that limit. In gauntlet mode a bonus life adds 15
  seconds to the clock instead.
- The score shown has six digits. After 999999 it starts again from 0 and no
  further bonus lives are given.

During a game:

- **Space** pauses. The top line shows `PRESS ANY KEY`; the next key or
  gamepad button continues the game. That key does nothing else, so pressing
  fire to continue does not also fire.
- The game also pauses when the window loses focus or a gamepad in use is
  unplugged. Every key and button counts as released, so Digger does not keep
  running in the direction you were holding.
- **+** (keypad) or **=** speeds the game up, **−** (keypad) or **-** slows
  it down, by 10 milliseconds per frame each. Speed does not change how a
  game turns out, and it is not saved.
- **F7** switches the music off and on; effects keep playing. **F9** switches
  all sound off and on.
- **F10** ends the game at once and returns to the title screen, without
  high-score entry.
- Closing the window works at any moment and saves everything that is
  pending: the record file, changed settings and the score table.

## Game modes

N on the title screen cycles through the modes in this order; the label
under the title and the high-score table change with it.

| Mode | Title label | What it is |
|---|---|---|
| One player | `ONE` / ` PLAYER ` | One player, one Digger. |
| Two players | `TWO` / ` PLAYERS` | Two players take turns with one Digger. Both use the player 1 keys. |
| Two players simultaneous | `TWO PLAYER` / `SIMULTANEOUS` | Two Diggers on the screen at once; the second uses the player 2 keys or the second gamepad. |
| Gauntlet | `GAUNTLET` / `MODE` | One Digger against the clock (2:00 by default, see `--gauntlet` and `game.gauntlet_time`). Dying costs no life; the game ends with `TIME UP`. |
| Two-player gauntlet | `TWO PLAYER` / `GAUNTLET` | Two Diggers at once against the clock. |

The mode the title screen opens with comes from `--players`, `--two-diggers`
and `--gauntlet` when any of them is given, else from `game.mode` in the
settings file. A mode chosen with N is saved when the program ends.

## Keys

Keys are named by their position on the keyboard, not by the letter the
layout prints: the player 2 keys are the keys in the positions of W, A, S
and Z on a US keyboard, whatever your layout calls them. The names are the
W3C UI Events key codes, which the settings file uses as well.

| Action | Setting | Default keys |
|---|---|---|
| Player 1 right, up, left, down | `p1_right`, `p1_up`, `p1_left`, `p1_down` | `ArrowRight`, `ArrowUp`, `ArrowLeft`, `ArrowDown` |
| Player 1 fire | `p1_fire` | `F1` |
| Player 2 right, up, left, down | `p2_right`, `p2_up`, `p2_left`, `p2_down` | `KeyS`, `KeyW`, `KeyA`, `KeyZ` |
| Player 2 fire | `p2_fire` | `Tab` |
| Speed up | `speed_up` | `NumpadAdd`, `Equal` |
| Slow down | `speed_down` | `NumpadSubtract`, `Minus` |
| Music on/off | `toggle_music` | `F7` |
| Sound on/off | `toggle_sound` | `F9` |
| Quit | `quit` | `F10` |
| Pause | `pause` | `Space` |
| Change mode | `change_mode` | `KeyN` |
| Save recording | `save_recording` | `F8` |
| Redefine keys | `redefine_keys` | `KeyK` |
| Fullscreen | `fullscreen` | `F11` (Alt+Enter always works too) |

How the direction keys are read:

- When several direction keys of one player are held, the one pressed last
  counts. Keys pressed within the same frame rank right, then left, then
  down, then up.
- When the key in use is released while others are held, the direction
  falls back to a held key in the order right, left, down, up.
- A tap shorter than a frame still moves Digger for one step, and a short tap
  on fire still fires once.
- The player 2 keys only work in the two simultaneous modes. In the
  alternating two-player mode both players use the player 1 keys.
- Function keys act once per press; holding one down or keyboard auto-repeat
  does not repeat it.

Valid key names are the letters `KeyA` … `KeyZ`, the digits `Digit0` …
`Digit9`, `F1` … `F24`, `Numpad0` … `Numpad9`, `NumpadAdd`,
`NumpadSubtract`, `NumpadMultiply`, `NumpadDivide`, `NumpadDecimal`,
`NumpadComma`, `NumpadEnter`, `NumpadEqual`, `NumLock`, `ArrowUp`,
`ArrowDown`, `ArrowLeft`, `ArrowRight`, `Backquote`, `Backslash`,
`BracketLeft`, `BracketRight`, `Comma`, `Equal`, `IntlBackslash`, `IntlRo`,
`IntlYen`, `Minus`, `Period`, `Quote`, `Semicolon`, `Slash`, `AltLeft`,
`AltRight`, `Backspace`, `CapsLock`, `ContextMenu`, `ControlLeft`,
`ControlRight`, `Enter`, `MetaLeft`, `MetaRight`, `ShiftLeft`, `ShiftRight`,
`Space`, `Tab`, `Convert`, `KanaMode`, `Lang1`, `Lang2`, `NonConvert`,
`Delete`, `End`, `Help`, `Home`, `Insert`, `PageDown`, `PageUp`, `Escape`,
`PrintScreen`, `ScrollLock` and `Pause`.

## Gamepads

Gamepads work without any set-up. The first pad connected is player 1, the
second is player 2; further pads are ignored. Pads can be plugged in and
out at any time; a newly connected pad takes the lowest free player number.
The keyboard keeps working next to the pads.

| Control | Default |
|---|---|
| Move | d-pad, or the left stick pushed beyond the dead zone (half of full deflection) |
| Fire | `South` and `West` (the bottom and left face buttons: A and X on an Xbox pad, cross and square on a PlayStation pad) |
| Pause | `Start` |
| Change mode | `Select` (Back or View on some pads) |
| Quit | none |

On the title screen `South`, `West` and `Start` start a game. Any button ends
a pause. Unplugging a pad during a game pauses it.

During initials entry, up and down on the d-pad or stick step through the
letters A to Z, a fire button takes the letter and d-pad left steps back one
place.

The buttons for fire, pause, change mode and quit, the dead zone, and
whether pads are used at all are set in the `[gamepad]` table of the
settings file. Button names are `South`, `East`, `North`, `West`, `C`, `Z`,
`LeftTrigger`, `LeftTrigger2`, `RightTrigger`, `RightTrigger2`, `Select`,
`Start`, `Mode`, `LeftThumb` and `RightThumb`. Movement cannot be remapped.

## Redefining the keys

Press **K** on the title screen, or start the program with
`--redefine-keys`. The screen says `PRESS NEW KEY FOR` and asks for player
1's `RIGHT`, `UP`, `LEFT`, `DOWN` and `FIRE` keys in turn; in a mode with two
Diggers it then asks for the same five keys of player 2. The action being
asked for is highlighted until a key is taken for it. The key you press
becomes that action's only key.

- A key already chosen for an earlier action is refused and the same action
  is asked again.
- After the player keys, every function key (pause, speed, sound, …) whose
  key you have just taken is asked for again. `--redefine-keys=all` asks for
  every function key in any case (`ACCEL`, `BRAKE`, `MUSIC`, `SOUND`,
  `EXIT`, `PAUSE`, `MODE CHANGE`, `SAVE DRF`, `REDEFINE`, `FULLSCREEN`).
- In a one-Digger mode, if you take one of player 2's keys for player 1,
  player 2 gets player 1's old key for the same direction (with the defaults,
  taking `KeyA` for left gives player 2 `ArrowLeft`).
- The Alt keys cannot be chosen.
- The quit key (F10 by default) abandons the redefinition: all keys stay as
  they were and nothing is saved.

A completed redefinition is written to the settings file at once and is in
effect from then on, also in later runs.

## High scores

There are four tables of ten entries: normal and gauntlet games, each with
one Digger or with two. One-player and alternating two-player games share
the one-Digger normal table. The title screen shows the table of the
selected mode.

A score enters a table when it is higher than the tenth entry; a tie with
an entry goes above it. The screen then shows ` NEW HIGH SCORE ` and asks
for three initials. Only the letters A to Z are taken (shown in capitals);
Backspace or Delete steps back. With two Diggers, each Digger's score is
considered in turn. The flashing of this screen can be switched off with
`video.reduce_flashing = true`.

Playbacks of recordings, games taken over from a playback, and games ended
with the quit key or by closing the window never enter the tables. Games
with unlimited lives do.

Games played with a level file have their own four tables, named after the
level file.

## Command line

```
digger [OPTIONS] [SPEED|LEVELS]
```

Every option also has a value in the settings file or is a one-off; options
given on the command line win over the settings file and are never written
to it. Option order does not matter.

The upstream one-letter options are accepted as aliases: with `/` or `-`, in
either case, with the value after a colon (`/S:40`, `-s:40`). A number may
follow the letter directly (`/S40`, `-s40`). A file option needs its colon
(`/L:levels.dlf`), so an absolute path such as `/Users/ann/levels.dlf` is
taken as a file name and not as an option. `/V` is accepted and does
nothing. `/C` (CGA graphics), `/N` (networked play) and `/X` (embedding in
an X11 window) are refused with `digger: error: option /C is not supported:
…` and exit status 64.

<!-- table:options -->
| Option | Upstream alias | Meaning |
|---|---|---|
| `--speed <N>` | `/S:N` | Game speed 0–500: the frame time is N × 2000 µs (default 40, which is 80 ms or 12.5 frames per second); 0 runs as fast as the machine can. |
| `--start-level <N>` | `/I:N` | The level games start on, 1–1000. |
| `--unlimited-lives` | `/U` | No limit of five lives for bonus lives. |
| `--players <1\|2>` | — | 2: two players take turns. |
| `--two-diggers` | `/2` | Two players at once with two Diggers. |
| `--gauntlet[=<SECS>]` | `/G[:SECS]` | Gauntlet mode, with the clock starting at SECS seconds (1–3599); without a value, or with 0, the `game.gauntlet_time` setting. With `--two-diggers`: two-player gauntlet. |
| `--quiet` | `/Q` | Start with all sound off. F9 switches it on. |
| `--no-music` | `/M` | Start with the music off. F7 switches it on. |
| `--fullscreen` | `/F` | Start fullscreen. |
| `--scale <MODE>` | — | How the picture fills the window: `integer`, `fit` or `stretch` (see `video.scale`). |
| `--redefine-keys[=<all>]` | `/K, /KA` | Open key redefinition before the title screen; `=all` (or `/KA`) also asks for every function key. |
| `--levels <FILE>` | `/L:FILE` | Play the levels of a level file. If FILE does not exist and has no extension, `FILE.dlf` is tried. |
| `--record <FILE>` | `/R:FILE` | Save every game played to FILE when it ends, replacing what was there. A name ending in `.edrf` selects the extended format. |
| `--play <FILE>` | `/P:FILE` | Play a recording in the window before the title screen. May be given several times. |
| `--loop` | `/O` | Repeat the `--play` list until F10. |
| `--take-over-at <N>` | `/T:N` | With `--play`: after tick N of the playback, the game is yours to continue. |
| `--verify <FILE>` | `/E:FILE` | Play one recording and exit with its result (see [Exit statuses](#exit-statuses)). |
| `--headless` | — | With `--verify`: no window, no sound, as fast as possible. |
| `--config <FILE>` | — | Read and save settings in FILE instead of the usual settings file. |
| `--data-dir <DIR>` | — | Keep scores and recordings in DIR instead of the usual data directory. |
| `-h, --help` | `/?, /H` | Print the list of options and exit. |
| `-V, --version` | — | Print `digger` and the version number and exit. |
| `[SPEED\|LEVELS]` | — | A single bare argument: digits are the speed (as `--speed`), anything else a level file (as `--levels`). |
<!-- /table:options -->

These combinations are refused with exit status 64 and a message naming both
options: `--players 2` with `--two-diggers` or `--gauntlet`; `--verify` with
`--play`, `--loop` or `--take-over-at`; `--headless` without `--verify`;
`--loop` or `--take-over-at` without `--play`; a bare speed with `--speed`,
or a bare level file with `--levels`. Values that are not numbers or are out
of range are refused the same way; the message quotes the option as you
typed it (`/I`, not `--start-level`).

Examples:

```
digger --speed 30 --start-level 5        # faster, from level 5
digger /S:30 /I:5                        # the same, upstream style
digger --two-diggers --gauntlet=300      # two-player gauntlet, 5 minutes
digger --play game.drf --loop            # screensaver: replay until F10
digger --verify game.edrf --headless     # check a recording on a server
```

## Exit statuses

| Status | Meaning |
|---|---|
| 0 | Normal end; or a `--verify` run that played the recording to its end in agreement with it |
| 1 | `--verify`: the file cannot be opened or is not a recording |
| 2 | `--verify`: the game went a different way than the recording (a checkpoint does not match) or a record is invalid |
| 3 | `--verify`: the playback was stopped before its end (F10 or closing the window) |
| 4 | `--verify`: an extended recording lacks its end marker |
| 64 | Command-line error: unknown or refused option, bad value, refused combination |
| 66 | The level file cannot be read or is shorter than 1202 bytes |
| 69 | No display can be opened |
| 78 | The settings file is not valid (see [Settings file](#settings-file)) |

A `--verify` run that ends with 0 prints exactly one line on standard
output: `score=<S> level=<L> ticks=<T>`, or `score=<S> score2=<S2>
level=<L> ticks=<T>` when the game had two players or two Diggers. Errors
go to standard error as `digger: error: …`, and things that do not stop the
program as `digger: notice: …` or `digger: warning: …`.

## Settings file

The settings live in `config.toml` (see [Where files are
kept](#where-files-are-kept) for the folder; `--config` names another file).
The program runs with the defaults when the file does not exist, and does
not create it just by running.

It writes the file only when you complete a key redefinition, and when the
program ends after you changed sound, music, fullscreen or the game mode with
their keys. Only those settings are updated in the file; options from the
command line and speed changes made with the speed keys are never saved. The
first save writes a complete file with every setting. **Comments you add to
the file by hand are lost when the program saves it.**

A setting given on the command line wins over the file, which wins over the
default. When any of `--players`, `--two-diggers` or `--gauntlet` is given,
`game.mode` from the file is not used.

A file that is not valid TOML, or holds a value of the wrong type, out of
range, an unknown key or button name, or one key bound to two actions, stops
the program before the window opens with exit status 78 and a message such
as `config.toml: sound.sample_rate: 999 is outside 8000–192000`. A setting
the program does not know is only warned about and ignored.

Every setting:

<!-- table:settings -->
| Setting | Values | Default | Meaning |
|---|---|---|---|
| `game.speed` | 0–500 | `40` | Frame time = speed × 2000 µs; 0 = as fast as possible. |
| `game.mode` | `one-player` \| `two-players` \| `two-diggers` \| `gauntlet` \| `two-digger-gauntlet` | `"one-player"` | The mode the title screen opens with. |
| `game.gauntlet_time` | 1–3599 | `120` | Seconds on the clock at the start of a gauntlet game. |
| `game.start_level` | 1–1000 | `1` | The level games start on. |
| `game.unlimited_lives` | true or false | `false` | No limit of five lives for bonus lives. |
| `sound.enabled` | true or false | `true` | Sound at start. |
| `sound.music` | true or false | `true` | Music at start. |
| `sound.sample_rate` | 8000–192000 | `44100` | Output rate in Hz. If the device refuses it, its own rate is used and a notice says so. |
| `sound.buffer_size` | 64–8192 | `512` | Device buffer in frames (512 is about 11.6 ms at 44100 Hz). |
| `sound.volume` | 0–100 | `100` | Master volume in percent. |
| `video.fullscreen` | true or false | `false` | Start fullscreen. |
| `video.scale` | `integer` \| `fit` \| `stretch` | `"integer"` | `integer`: largest whole-number scale, centred on black. `fit`: as large as fits with the 8:5 shape kept. `stretch`: fill the window. |
| `video.reduce_flashing` | true or false | `false` | `true`: the new-high-score screen does not flash. |
| `keys.p1_right` | list of key names | `["ArrowRight"]` | Player 1 right. |
| `keys.p1_up` | list of key names | `["ArrowUp"]` | Player 1 up. |
| `keys.p1_left` | list of key names | `["ArrowLeft"]` | Player 1 left. |
| `keys.p1_down` | list of key names | `["ArrowDown"]` | Player 1 down. |
| `keys.p1_fire` | list of key names | `["F1"]` | Player 1 fire. |
| `keys.p2_right` | list of key names | `["KeyS"]` | Player 2 right. |
| `keys.p2_up` | list of key names | `["KeyW"]` | Player 2 up. |
| `keys.p2_left` | list of key names | `["KeyA"]` | Player 2 left. |
| `keys.p2_down` | list of key names | `["KeyZ"]` | Player 2 down. |
| `keys.p2_fire` | list of key names | `["Tab"]` | Player 2 fire. |
| `keys.speed_up` | list of key names | `["NumpadAdd", "Equal"]` | Shorten the frame time by 10000 µs. |
| `keys.speed_down` | list of key names | `["NumpadSubtract", "Minus"]` | Lengthen the frame time by 10000 µs. |
| `keys.toggle_music` | list of key names | `["F7"]` | Music on or off. |
| `keys.toggle_sound` | list of key names | `["F9"]` | All sound on or off. |
| `keys.quit` | list of key names | `["F10"]` | End the game, or the program on the title screen. |
| `keys.pause` | list of key names | `["Space"]` | Pause the game. |
| `keys.change_mode` | list of key names | `["KeyN"]` | Next game mode, on the title screen. |
| `keys.save_recording` | list of key names | `["F8"]` | Save the last game's recording, on the title screen. |
| `keys.redefine_keys` | list of key names | `["KeyK"]` | Open key redefinition, on the title screen. |
| `keys.fullscreen` | list of key names | `["F11"]` | Window or fullscreen. |
| `gamepad.enabled` | true or false | `true` | `false`: gamepads are ignored. |
| `gamepad.dead_zone` | 0.05–0.95 | `0.5` | How far, as a fraction of full deflection, the stick must be pushed to count. |
| `gamepad.fire` | list of button names | `["South", "West"]` | Fire; also starts a game. |
| `gamepad.pause` | list of button names | `["Start"]` | Pause; also starts a game. |
| `gamepad.change_mode` | list of button names | `["Select"]` | Next game mode, on the title screen. |
| `gamepad.quit` | list of button names | `[]` | End the game, or the program on the title screen. |
<!-- /table:settings -->

Each `[keys]` entry needs at least one key; an action left out of the file
keeps its default keys. A key can belong to one action only. A button list
may be empty.

An example file that changes a few settings:

```toml
[game]
speed = 30
mode = "gauntlet"
gauntlet_time = 90

[sound]
volume = 50

[keys]
p1_fire = ["ControlLeft", "F1"]

[gamepad]
quit = ["Mode"]
```

The program does not read upstream Digger's `.digger.rc` or `DIGGER.INI`.

## Where files are kept

| | Linux | Windows | macOS |
|---|---|---|---|
| Settings file `config.toml` | `$XDG_CONFIG_HOME/digger/`, or `~/.config/digger/` | `%APPDATA%\digger\config\` | `~/Library/Application Support/digger/` |
| Score file `scores.toml` | `$XDG_DATA_HOME/digger/`, or `~/.local/share/digger/` | `%APPDATA%\digger\data\` | `~/Library/Application Support/digger/` |
| Recordings saved with F8 | `recordings/` in the score file's folder | the same | the same |

In a web browser nothing is written to files: settings and scores are kept
in the browser's local storage for the site the game is served from (see
[Playing in a web browser](#playing-in-a-web-browser)), and a recording
saved with F8 is a download, which goes wherever the browser puts
downloads.

`--config <FILE>` replaces the settings file, `--data-dir <DIR>` the folder
of the score file and of the `recordings` folder.

The score file is this program's own text format and starts with a format
version, currently 1. It is written each time an entry is added, in a way
that leaves either the old or the new complete file if the program is
interrupted. If it cannot be written, a notice says so and the new entry is
kept until the program ends. A score file that cannot be read, or that comes
from a newer version, gives empty tables and a notice; before it is next
written it is renamed to `scores.toml.bad`, so nothing is lost. Upstream's
`.digger.sco` and `DIGGER.SCO` are not read.

## Recordings

Every game can be recorded and played back exactly. Two formats exist, both
upstream Digger's own and readable by it:

- **Classic** (`.drf`): the player's moves.
- **Extended** (`.edrf`): the same, plus state checkpoints that let a
  verification detect the exact moment a playback goes a different way.

Ways to make a recording:

- `--record <FILE>` saves every game played in that run to FILE when the
  game ends (also when it is ended with F10 or by closing the window). Each
  game replaces the previous one in the file. A name ending in `.edrf` gives
  the extended format.
- Without `--record`, **F8** on the title screen saves the last game played
  in the run, once, in the `recordings` folder of the data directory, named
  after the initials entered and the score, for example `ABC12300.drf`
  (`___` stands for no initials; `-2`, `-3`, … is added when the name is
  taken).

Ways to watch or check one:

- `--play <FILE>` shows the recording in the window before the title screen
  appears, with picture and sound at the current speed; several `--play`
  options are played in order. Movement and fire keys are ignored; pause,
  speed, sound, music, fullscreen and quit work. The mode, start level and
  levels come from the recording. A file that is not a recording is skipped
  with a notice.
- `--loop` repeats the list until F10, which shows the title screen.
- `--take-over-at <N>` hands the game being played back to you after tick N:
  from the next tick your keys drive it. The game then goes on as a live
  game and is saved by `--record` in full, but it never enters the high-score
  tables.
- `--verify <FILE>` plays one recording and exits with 0 to 4 (see [Exit
  statuses](#exit-statuses)), printing the result line on success. Add
  `--headless` to run without a window, for scripts and build servers. With
  `--record`, the replayed game is written to that file, so
  `digger --record game.edrf --verify game.drf --headless` converts a classic
  recording to the extended format.

## Level files

A level file (`.dlf`, upstream's format) holds the score step for bonus lives
(2 bytes) followed by eight level maps (1200 bytes). `--levels <FILE>`, or a
bare file name, plays them in place of the built-in levels. A missing,
unreadable or too-short file stops the program with exit status 66. Scores
made with a level file are kept in their own tables.

## Playing in a web browser

The web version is the same game built for WebAssembly: the same levels,
rules, keys, gamepad mapping, key redefinition and high-score tables. It
runs in the two most recent versions of Chrome, Edge, Firefox and Safari on
a desktop computer, and needs a keyboard or a gamepad; on a device with
only a touch screen the page says so (the title screen still runs). The
whole game is less than 1 MiB to download; once the title screen shows, it uses
no network.

The page shows the game picture, scaled up by the largest whole number that
fits the browser window, and under it buttons for *Fullscreen*, *Play
recording…* and *Save recording*, a table of the keys in effect (it follows
key redefinition) and a footer with the version, the licence and a link to
the source code of that version.

**Sound.** Browsers allow sound only after you have pressed something on
the page, so the game starts silent with the line *Press a key or click to
turn sound on* below the picture. The first key press, click or gamepad
button turns sound on and also counts as game input: pressing the key that
starts a game starts it with sound. Music already playing goes on from where
it has got to. If the browser offers no sound output, the line is replaced
by a notice and the game plays silently.

**Keys.** Click the picture, or press a key, to give the game the keyboard.
While it has the keyboard, the game's keys no longer do what the browser
would do with them (arrows and Space do not scroll, F1 opens no help).
Clicking elsewhere, such as the address bar, releases every held key.
Differences from the desktop program:

- **F10** on the title screen does nothing: a web page is never closed by
  the game. During a game F10 still ends it and returns to the title screen.
- **F11** or **Alt+Enter**, or the *Fullscreen* button, uses the browser's
  fullscreen mode; **Escape** leaves it, and the game goes on.
- Switching to another tab or minimising the window stops the game at once
  and silences it. A game in progress is then paused as with Space and
  continues when you press a key; the title screen and a recording being
  played go on by themselves when the page is shown again. Hidden time is
  never made up, so a gauntlet clock loses nothing.

**Options in the address.** In place of a command line, the page reads
these parameters after `?` in its address, joined with `&`. They apply to
that visit only and are never saved; without them, the saved settings apply.

| Parameter | Meaning |
|---|---|
| `speed=<N>` | Game speed 0–500, as `--speed` (`speed=0` runs as fast as the browser can) |
| `level=<N>` | The level games start on, 1–1000, as `--start-level` |
| `mode=<MODE>` | The mode the title screen opens with: `one-player`, `two-players`, `two-diggers`, `gauntlet` or `two-digger-gauntlet` |
| `quiet` or `quiet=1` | Start with all sound off and no sound line; F9 switches sound on |

For example `index.html?mode=gauntlet&speed=30` opens in gauntlet mode at
speed 30, and `?level=5&quiet` starts games on level 5 without sound. A
value that is not allowed, such as `speed=fast`, is ignored with a notice
naming the parameter and the value; parameters the game does not know are
ignored silently.

**Recordings.** After a game, **F8** on the title screen or the *Save
recording* button downloads its recording, under the same name the desktop
program would give it (for example `ABC12300.drf`) and with the same
content, so the desktop program replays it to the same score. The button is
greyed out until a game has been played. *Play recording…* opens a file
picker for `.drf` and `.edrf` files; the chosen file is read on your
computer, never uploaded, and played back as with `--play`, and the title
screen returns at its end. A file that is not a recording, of a kind not
supported or larger than 16 MiB is refused with a notice saying why.

**Saved settings and scores.** Settings (sound, music, mode, keys, …) and
the high-score tables are kept in the browser's local storage, under the
keys `digger:settings` and `digger:scores`, and are written at once when
they change. They belong to the site address the game was opened from and
are removed when you clear that site's data. If the browser refuses
storage (some private windows, or site data blocked in the settings), a
notice says that settings and scores will not be kept: the game plays
normally and remembers them until the page is closed.

**Putting it on a web server.** Unpack `digger-<version>-web.tar.gz` and
copy the contents of its `site` folder to any static web server, at the
site root or in a sub-folder; the files refer to each other by relative
addresses. The server must send `.wasm` files as `application/wasm`. From
a source checkout, `cargo xtask web --serve` builds the same files into
`dist/web/` and serves them at `http://127.0.0.1:8000/`; `--port <N>` and
`--prefix <PATH>` choose another port or a sub-folder, for example
`cargo xtask web --serve --port 8080 --prefix /games/digger/` serves the
game at `http://127.0.0.1:8080/games/digger/`. Opening `index.html` straight
from a disk folder does not work: browsers load WebAssembly only over HTTP.

## Differences from upstream Digger

This program behaves like upstream Digger Remastered wherever the course of
a game is concerned: recordings made by either replay identically in the
other. It differs in the program around the game:

- **Not available:** CGA graphics (`/C`), networked play (`/N`), embedding
  in an X11 window (`/X`) and the cheat key.
- **Own files:** settings in `config.toml` and scores in `scores.toml`, in
  the platform's usual folders. Upstream's `.digger.rc`, `DIGGER.INI`,
  `.digger.sco` and `DIGGER.SCO` are neither read nor written.
- **Command line:** long options, with the upstream letters as aliases.
  Values are checked instead of silently wrapped or clamped, option order
  does not matter, and an absolute path is not mistaken for an option.
  Errors exit with 64, 66, 69 or 78 rather than 1, so that statuses 1 to 4
  keep their verification meaning; `--help` exits with 0. The
  verification result line is always printed, without upstream's
  `DIGGER_CI_RUN` variable.
- **Sound:** sound can be switched on with F9 after a quiet start, and the
  volume setting works.
- **Keys:** keys are matched by position, so they work on any layout. Each
  function key acts once per press. On the title screen, the sound, music,
  speed, save and fullscreen keys no longer start a game. Added: F11 and
  Alt+Enter for fullscreen, `=` and `-` as speed keys.
- **Key redefinition:** abandoning it with the quit key restores every key,
  and a key can never end up bound to two actions.
- **Initials:** only the letters A to Z are taken and shown in capitals; the
  arrow keys no longer type letters, and Backspace works at any moment.
- **Saving a recording with F8:** the file goes to the `recordings` folder,
  keeps its full name and never overwrites an earlier one.
- **Pause:** also pauses when the window loses focus or a gamepad in use is
  unplugged.
- **New:** gamepads, a resizable window with integer scaling and fullscreen,
  `--take-over-at` as an option of its own, `--headless` verification, and
  the `reduce_flashing` setting.
- **Faithful to upstream:** the speed keys stop at a 10 ms frame time, the
  second player's bonus life comes one point late, the score restarts from 0
  after 999999, and the other quirks listed in
  [`compat-quirks.md`](compat-quirks.md).
