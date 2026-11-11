# Spec Delta

## Purpose

How the program is told what to do: the command line with its long options and upstream-compatible aliases, the configuration file with every setting and its default, and the rules for combining, validating and saving them.

## ADDED Requirements

### Requirement: Game set-up options
The command line SHALL accept `--speed <N>` (0 to 500; frame time N × 2000 µs, 0 = unpaced), `--start-level <N>` (1 to 1000) and `--unlimited-lives`. A single bare argument made only of digits SHALL be taken as the speed. Option order SHALL NOT matter. (research 01 §1.1, research 05 §3; the ranges and order independence are port decisions)

#### Scenario: Speed and start level
- **WHEN** the program is started with `--speed 30 --start-level 5`
- **THEN** games start on level 5 with a frame time of 60000 µs

#### Scenario: Bare number
- **WHEN** the program is started as `digger 60`
- **THEN** the frame time is 120000 µs

### Requirement: Mode options
The command line SHALL accept `--players <1|2>`, `--two-diggers` and `--gauntlet[=<seconds>]` (1 to 3599; without a value, or with 0, the configured gauntlet time). They select the mode the title screen opens with: none of them leaves the configured mode; `--players 2` gives two alternating players; `--two-diggers` gives two players simultaneous; `--gauntlet` gives gauntlet; both together give two-player gauntlet. (research 01 §1.1 and §2.1)

#### Scenario: Gauntlet with a time
- **WHEN** the program is started with `--gauntlet=300`
- **THEN** the title screen shows `GAUNTLET` over `MODE` and a game starts with 5:00 on the clock

#### Scenario: Two-player gauntlet
- **WHEN** the program is started with `--two-diggers --gauntlet`
- **THEN** the title screen shows `TWO PLAYER` over `GAUNTLET` and the clock starts at the configured gauntlet time, 2:00 by default

### Requirement: Sound and display options
The command line SHALL accept `--quiet` (start with all sound off), `--no-music` (start with music off), `--fullscreen` (start fullscreen), `--scale <integer|fit|stretch>` and `--redefine-keys[=all]` (open key redefinition before the title screen). `--quiet` and `--no-music` set starting states only; the toggles still work. (research 04 §6, research 05 §3; `--scale` is a port decision)

#### Scenario: Quiet start
- **WHEN** the program is started with `--quiet`
- **THEN** no sound is heard until the player uses the sound toggle

### Requirement: Level file option
`--levels <file>`, or a single bare argument that is not a number, SHALL load the eight level layouts and the bonus-life score from a level file of at least 1202 bytes (2 bytes of score, then 1200 bytes of maps). If the file does not exist and the name has no extension, `<file>.dlf` SHALL be tried. An unreadable or too-short file SHALL end the program with a message naming it and exit status 66. (research 01 §1.1, research 05 §3; failing loudly is a port decision)

#### Scenario: Missing level file
- **WHEN** the program is started with `--levels nosuch`
- **THEN** it prints `digger: error: cannot read level file` followed by the name on standard error and exits with status 66 without opening a window

#### Scenario: Extension added
- **WHEN** `--levels mylevels` is given and only `mylevels.dlf` exists
- **THEN** `mylevels.dlf` is loaded

### Requirement: Record option
`--record <file>` SHALL save every game played by a person to that file when the game ends, replacing the previous content. A name ending in `.edrf` (any letter case) selects the extended recording format; any other name selects the classic format. Without `--record`, the save-recording action on the title screen saves the last game as `<initials><score>.drf` in the `recordings` folder of the data directory. (research 05 §3 and §4.5; the folder is a port decision)

#### Scenario: Recording a game
- **WHEN** the program is started with `--record run.edrf` and the player plays one game to its end
- **THEN** `run.edrf` exists as an extended recording that replays to the same score

#### Scenario: Saving the last game by key
- **WHEN** the player ends a game with 12300 points, enters `ABC` and presses F8 on the title screen
- **THEN** `ABC12300.drf` is written in the `recordings` folder, and a second F8 writes nothing

### Requirement: Play options
`--play <file>` SHALL play a recording in the window before the title screen appears; it can be given several times and the recordings are played in the order given. `--loop` SHALL repeat the list until the quit action or the closing of the window. A file that cannot be opened or is not a recording SHALL be reported on standard error and skipped. Mode, start level and levels come from the recording; `--unlimited-lives` is not applied. (research 01 §1.1, research 05 §2.3 and §3)

#### Scenario: Play then title
- **WHEN** the program is started with `--play a.drf --play b.edrf`
- **THEN** both games are shown in that order and the title screen follows

#### Scenario: Screensaver loop
- **WHEN** the program is started with `--play a.drf --loop`
- **THEN** the recording restarts each time it ends until the player presses F10, which ends the program

### Requirement: Play-and-verify
`--verify <file>` SHALL play one recording and exit with status 0 if it ran to its end in agreement with the recording, 1 if the file cannot be opened or is not a recording, 2 if the game diverged from a checkpoint or a record is invalid, 3 if playback was stopped before the end, and 4 if an extended recording lacks its end marker. With `--record`, the replayed game is saved to that file before exiting. (research 01 §1.1, research 05 §3)

#### Scenario: Good recording
- **WHEN** `digger --verify sobodig.drf --speed 0` is run
- **THEN** the exit status is 0

#### Scenario: Not a recording
- **WHEN** `--verify` names a text file that does not start as a recording
- **THEN** a message naming the file is printed on standard error and the exit status is 1

#### Scenario: Stopped by the user
- **WHEN** the player presses F10 halfway through a verification run
- **THEN** the exit status is 3

#### Scenario: Converting a recording
- **WHEN** `digger --record game.edrf --verify game.drf` is run and ends with status 0
- **THEN** `game.edrf` holds the same game in the extended format

### Requirement: Verification result line
A verification run that ends with status 0 SHALL print exactly one line on standard output: `score=<S> level=<L> ticks=<T>` or, when the game had two players or two Diggers, `score=<S> score2=<S2> level=<L> ticks=<T>`. Scores are full totals; ticks counts simulated ticks. Other statuses print nothing on standard output. (research 05 §7.1–7.2; printing without upstream's environment variable is a port decision)

#### Scenario: One-player result
- **WHEN** a recording of a one-player game ending with 3100 points on level 2 after 1378 ticks is verified
- **THEN** standard output is the single line `score=3100 level=2 ticks=1378`

#### Scenario: Two-digger result
- **WHEN** a two-Digger recording ending with 18100 and 1100 points on level 4 after 4489 ticks is verified
- **THEN** standard output is `score=18100 score2=1100 level=4 ticks=4489`

### Requirement: Verifying without a window
`--headless`, allowed only together with `--verify`, SHALL run the verification without opening a window or an audio device and as fast as possible, with the same exit statuses and result line. (port decision)

#### Scenario: Headless on a build server
- **WHEN** `digger --verify sobodig.edrf --headless` is run on a machine without a display
- **THEN** the result line is printed and the exit status is 0

### Requirement: Upstream option aliases
The command line SHALL also accept upstream's single-letter options, with `/` or `-`, in either letter case, value after `:`: `/S:n` speed, `/I:n` start level, `/G[:secs]` gauntlet, `/2` two diggers, `/U` unlimited lives, `/Q` quiet, `/M` no music, `/F` fullscreen, `/L:file` levels, `/R:file` record, `/P:file` play, `/E:file` verify, `/O` loop, `/K`, `/KA` redefine keys, `/?`, `/H` help. `/V` is ignored. A number may follow the letter directly (`/S40`). (research 01 §1.1, research 05 §3)

#### Scenario: Classic command line
- **WHEN** the program is started as `digger /S:40 /Q /G:120 /2 /U /I:3`
- **THEN** it behaves exactly as with `--speed 40 --quiet --gauntlet=120 --two-diggers --unlimited-lives --start-level 3`

#### Scenario: Dash and lower case
- **WHEN** the program is started as `digger -e:game.drf -s0`
- **THEN** it behaves as with `--verify game.drf --speed 0`

#### Scenario: Absolute path is not an option
- **WHEN** the program is started as `digger /Users/ann/levels.dlf` on macOS
- **THEN** the argument is taken as a level file name, not as the `/U` option

### Requirement: Unsupported upstream options
The upstream options for features this port does not have SHALL be refused: `/C` (CGA graphics), `/N` (networked play), `/X` (embedding in an X11 window) and `/T` (taking over a playback). The program SHALL print `digger: error: option <opt> is not supported: <reason>` on standard error and exit with status 64 without opening a window. (port decision; porting plan non-goals)

#### Scenario: CGA requested
- **WHEN** the program is started as `digger /C`
- **THEN** standard error carries `digger: error: option /C is not supported: CGA graphics are not part of this port` and the exit status is 64

#### Scenario: NetSim requested
- **WHEN** the program is started as `digger /N:host`
- **THEN** a not-supported message naming `/N` and networked play is printed and the exit status is 64

### Requirement: Help and version
`--help`, `-h`, `/?` and `/H` SHALL print a usage text listing every long option with its upstream alias and exit with status 0. `--version` and `-V` SHALL print `digger` followed by the version number and exit with status 0. Neither opens a window. An unknown option SHALL print a message naming it plus a hint to use `--help`, and exit with status 64. (research 01 §1.1; exit status 0 for help and `-V` for version are port decisions)

#### Scenario: Help
- **WHEN** `digger --help` is run
- **THEN** standard output lists `--speed <N>` together with its alias `/S:N`, and the exit status is 0

#### Scenario: Unknown option
- **WHEN** `digger --turbo` is run
- **THEN** standard error names `--turbo` and the exit status is 64

### Requirement: Configuration file location
Settings SHALL be read from `config.toml` in the per-user configuration directory: `$XDG_CONFIG_HOME/digger/` (or `~/.config/digger/`) on Linux, `%APPDATA%\digger\config\` on Windows, `~/Library/Application Support/digger/` on macOS. `--config <file>` names another file. A file that does not exist is not an error: defaults apply. (port decision)

#### Scenario: No file yet
- **WHEN** the program starts for the first time on Linux
- **THEN** it runs with default settings and does not create `~/.config/digger/config.toml`

#### Scenario: Explicit file
- **WHEN** the program is started with `--config ./test.toml` and that file sets `game.speed = 20`
- **THEN** the frame time is 40000 µs and the file in the configuration directory is not read

### Requirement: Game settings
The `[game]` table SHALL accept: `speed` (integer 0–500, default 40), `mode` (one of `"one-player"`, `"two-players"`, `"two-diggers"`, `"gauntlet"`, `"two-digger-gauntlet"`; default `"one-player"`), `gauntlet_time` (seconds 1–3599, default 120), `start_level` (1–1000, default 1) and `unlimited_lives` (boolean, default false). (research 05 §2.3; names and ranges are port decisions)

#### Scenario: Gauntlet by default
- **WHEN** the file contains `[game]` with `mode = "gauntlet"` and `gauntlet_time = 90`
- **THEN** the title screen opens in gauntlet mode and a game starts with 1:30 on the clock

### Requirement: Sound settings
The `[sound]` table SHALL accept: `enabled` (boolean, default true), `music` (boolean, default true), `sample_rate` (Hz, 8000–192000, default 44100), `buffer_size` (frames, 64–8192, default 512) and `volume` (percent, 0–100, default 100). (research 04 §6, research 05 §2.3; `volume` taking effect fixes an ignored upstream setting)

#### Scenario: Half volume
- **WHEN** the file sets `sound.volume = 50`
- **THEN** every sound is played at half the amplitude it has at 100

### Requirement: Video settings
The `[video]` table SHALL accept: `fullscreen` (boolean, default false), `scale` (`"integer"`, `"fit"` or `"stretch"`; default `"integer"`) and `reduce_flashing` (boolean, default false; when true the initials screen does not flash). (port decision)

#### Scenario: Stretch from the file
- **WHEN** the file sets `video.scale = "stretch"`
- **THEN** the picture fills the whole window

### Requirement: Key and gamepad settings
The `[keys]` table SHALL accept one entry per action name, each a list of one or more key names; actions left out keep their defaults. The `[gamepad]` table SHALL accept `enabled` (boolean, default true), `dead_zone` (0.05–0.95, default 0.5) and the button lists `fire` (default `["South", "West"]`), `pause` (`["Start"]`), `change_mode` (`["Select"]`) and `quit` (`[]`). (port decision)

#### Scenario: One action rebound in the file
- **WHEN** the file contains `[keys]` with `p1_fire = ["ControlLeft", "F1"]` and nothing else
- **THEN** both the left Control key and F1 fire, and all other actions keep their default keys

#### Scenario: Pads disabled
- **WHEN** the file sets `gamepad.enabled = false`
- **THEN** connected gamepads have no effect

### Requirement: Precedence
For every setting, a value given on the command line SHALL win over the value in the configuration file, which wins over the built-in default. If any of `--players`, `--two-diggers` or `--gauntlet` is given, the mode is determined by the command line alone and `game.mode` from the file is not consulted. (port decision)

#### Scenario: Command line beats file
- **WHEN** the file sets `game.speed = 20` and the program is started with `--speed 50`
- **THEN** the frame time is 100000 µs

#### Scenario: Mode option replaces the file's mode
- **WHEN** the file sets `game.mode = "gauntlet"` and the program is started with `--two-diggers`
- **THEN** the title screen opens in two players simultaneous mode, not in two-player gauntlet

### Requirement: Invalid configuration file
A configuration file that is not valid TOML, or that holds a value of the wrong type, out of range, an unknown key name or button name, or one key bound to two actions, SHALL end the program before a window opens with exit status 78 and a message naming the file and the offending key, for example `config.toml: sound.sample_rate: 999 is outside 8000–192000`. Unknown settings SHALL only cause a warning line and be ignored. (port decision)

#### Scenario: Out-of-range value
- **WHEN** the file sets `game.start_level = 0`
- **THEN** standard error names `game.start_level` and the value 0, and the exit status is 78

#### Scenario: Duplicate key binding
- **WHEN** the file binds `Space` to both `pause` and `p1_fire`
- **THEN** standard error names `Space` and both actions, and the exit status is 78

#### Scenario: Unknown setting
- **WHEN** the file contains `game.turbo = true`
- **THEN** a warning naming `game.turbo` is printed and the program starts normally

### Requirement: Invalid command line values
A command-line value that is not a number where one is needed, or is out of range, SHALL end the program with exit status 64 and a message naming the option and the value. This applies to upstream aliases too, reported under the form the user typed. (port decision; upstream silently wraps or clamps, research 01 §1.1)

#### Scenario: Speed out of range
- **WHEN** the program is started with `--speed 9000`
- **THEN** standard error names `--speed` and 9000 and the exit status is 64

#### Scenario: Alias with a bad value
- **WHEN** the program is started as `digger /I:abc`
- **THEN** standard error names `/I` and `abc` and the exit status is 64

### Requirement: Invalid option combinations
The program SHALL refuse, with exit status 64 and a message naming both options: `--players 2` with `--two-diggers` or with `--gauntlet` (there is no such game mode); `--verify` with `--play` or `--loop`; `--headless` without `--verify`; `--loop` without `--play`. (research 01 §1.2 describes the undefined upstream behaviour for the first case; rejecting is a port decision)

#### Scenario: Two players and two diggers
- **WHEN** the program is started with `--players 2 --two-diggers`
- **THEN** standard error says that `--players 2` cannot be combined with `--two-diggers`, and the exit status is 64

#### Scenario: Alias form of the same mistake
- **WHEN** the program is started as `digger --players 2 /G`
- **THEN** it is refused in the same way, naming `--players 2` and `/G`

### Requirement: Saving settings
The program SHALL write the configuration file only when a key redefinition completes, and at exit when the player changed sound, music, fullscreen or the game mode with in-game actions. Only those settings are updated; command-line values and speed-key changes are never written. The first save creates the directory and a complete file holding every setting. (research 05 §2.3 documents upstream saving keys only; the rest is a port decision)

#### Scenario: First save creates the file
- **WHEN** no configuration file exists, the player presses F7 once on the title screen and quits
- **THEN** `config.toml` exists, lists every setting with its default, and has `music = false` under `[sound]`

#### Scenario: Command-line values are not saved
- **WHEN** the program is started with `--speed 10 --quiet`, the player toggles fullscreen and quits
- **THEN** the saved file has `fullscreen = true`, and its `speed` and `enabled` entries are what they were before

#### Scenario: Nothing changed
- **WHEN** the player starts the program, plays a game and quits without using a toggle
- **THEN** the configuration file is not created or modified

### Requirement: Upstream files are not used
The program SHALL NOT read or write upstream's settings and score files (`.digger.rc`, `DIGGER.INI`, `.digger.sco`, `DIGGER.SCO`) and SHALL NOT react to their presence. (porting plan non-goal; research 05 §2.1)

#### Scenario: Old settings in the home directory
- **WHEN** `~/.digger.rc` sets `Speed=20000` and no `config.toml` exists
- **THEN** the program runs at the default frame time of 80000 µs and leaves `~/.digger.rc` untouched
