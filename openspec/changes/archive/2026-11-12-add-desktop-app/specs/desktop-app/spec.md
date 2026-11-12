# Spec Delta

## Purpose

The playable desktop program: it shows the game picture in a window, paces the game in real time, drives the audio device, and leads the player from the title screen through a game and back.

## ADDED Requirements

### Requirement: Supported desktop platforms
The game program, named `digger`, SHALL run on Linux under both X11 and Wayland, on Windows and on macOS, with the same behaviour on each. When no display can be opened it SHALL print one line naming the cause on standard error and exit with status 69. (port decision)

#### Scenario: Wayland session
- **WHEN** `digger` is started in a Wayland session without an X server
- **THEN** the game window opens and the title screen is shown

#### Scenario: No display available
- **WHEN** `digger` is started on Linux with neither a Wayland nor an X11 display reachable
- **THEN** it prints a message starting with `digger: error:` on standard error and exits with status 69

### Requirement: Game window
The program SHALL open one window titled `D I G G E R` with a default client area of 1280×800 physical pixels, resizable by the user down to 640×400. It SHALL use the game's 64×64 icon as the window icon on platforms that have per-window icons. (research 03 §10; default size and resizing are port decisions)

#### Scenario: Default window
- **WHEN** the program starts without a fullscreen setting
- **THEN** a window titled `D I G G E R` with a 1280×800 client area appears, showing the picture at scale 2

#### Scenario: Resize limit
- **WHEN** the user drags the window edge to make it smaller than 640×400
- **THEN** the client area stops shrinking at 640×400

### Requirement: Integer-scaled presentation
The program SHALL show the 640×400 game picture, expanded through the current 16-colour palette, at the largest whole-number scale that fits the client area, centred, with the remaining area black. Pixels SHALL be replicated without smoothing. This is the default scale mode, `integer`. (research 03 §10; integer scaling and letterboxing are port decisions)

#### Scenario: Window of 1920×1080
- **WHEN** the client area is 1920×1080 and the scale mode is `integer`
- **THEN** the picture is drawn at scale 2 as 1280×800, with 320 black columns on each side and 140 black rows above and below

#### Scenario: Window of 1300×1300
- **WHEN** the client area is 1300×1300
- **THEN** the picture is drawn at scale 2 as 1280×800 at offset (10, 250)

#### Scenario: Palette change
- **WHEN** the game switches palette, for example on entering bonus mode
- **THEN** the next presented frame shows every pixel in the new palette's colours

### Requirement: Other scale modes
The program SHALL offer two further scale modes. `fit` shows the picture at the largest size with an 8:5 width-to-height ratio that fits the client area, centred with black bars. `stretch` fills the whole client area. Both replicate pixels without smoothing. A client area smaller than 640×400 SHALL be handled as `fit` when the mode is `integer`. (research 03 §10 for stretch; the rest is a port decision)

#### Scenario: Fit in 1920×1080
- **WHEN** the scale mode is `fit` and the client area is 1920×1080
- **THEN** the picture is drawn as 1728×1080 with 96 black columns on each side

#### Scenario: Stretch
- **WHEN** the scale mode is `stretch` and the client area is 1920×1080
- **THEN** the picture covers all 1920×1080 pixels

#### Scenario: Integer mode in a tiny window
- **WHEN** the scale mode is `integer` and the client area is 500×400
- **THEN** the picture is drawn as 500×312 with 44 black rows above and below

### Requirement: Fullscreen
Alt+Enter, Alt+keypad Enter and the fullscreen action (default F11) SHALL each switch between the window and borderless fullscreen on the current monitor, at any time. The Alt key and the Enter of that chord SHALL NOT start a game, end a pause or otherwise reach the game. The mouse pointer SHALL be hidden while fullscreen. The program SHALL start fullscreen when the fullscreen setting is on. (research 05 §1.1, research 03 §10; F11 and borderless mode are port decisions)

#### Scenario: Alt+Enter on the title screen
- **WHEN** the player presses Alt+Enter on the title screen
- **THEN** the program becomes fullscreen and the title screen keeps running without starting a game

#### Scenario: Back to the window
- **WHEN** the program is fullscreen and the player presses F11
- **THEN** the window returns with its previous size and position and the pointer is visible again

#### Scenario: Start fullscreen
- **WHEN** the program is started with `--fullscreen`
- **THEN** the first picture shown is fullscreen

### Requirement: Frame pacing
The program SHALL advance the game by exactly one frame per frame time and present the picture once per frame. The frame time is the configured speed times 2000 µs, by default 80000 µs (12.5 frames per second). Frame k SHALL be due at start + k × frame time, so that timing errors of single frames do not accumulate. (research 01 §4, research 05 §6.3)

#### Scenario: No drift over ten minutes
- **WHEN** a game runs for 600 seconds of wall-clock time at the default speed without pause or stall
- **THEN** between 7499 and 7501 frames have been advanced

#### Scenario: Speed option
- **WHEN** the program is started with `--speed 20`
- **THEN** the frame time is 40000 µs and 25 frames are advanced per second

### Requirement: Recovery after a stall
When the program falls more than three frame times behind its schedule (for example while the window is being dragged, or after the computer sleeps), it SHALL restart the schedule from the current time instead of running the missed frames in a burst. (port decision)

#### Scenario: Two-second stall
- **WHEN** the program is blocked for 2 seconds during a game at the default speed and then continues
- **THEN** at most 3 frames are advanced immediately and later frames follow at 80000 µs intervals

### Requirement: Runtime speed control
The speed-up action SHALL shorten the frame time by 10000 µs, but only while the frame time is above 10000 µs. The slow-down action SHALL lengthen it by 10000 µs up to at most 1000000 µs. A change takes effect from the next frame and is not written to the configuration file. Changing the speed SHALL NOT change the outcome of a game. (research 05 §1.1 and §6.3; the upper limit is a port decision)

#### Scenario: Speeding up from the default
- **WHEN** the player triggers speed-up seven times at the default speed
- **THEN** the frame time is 10000 µs, and an eighth speed-up leaves it at 10000 µs

#### Scenario: Slowing down
- **WHEN** the player triggers slow-down once at the default speed
- **THEN** the frame time is 90000 µs

### Requirement: Unpaced mode
With speed 0 the program SHALL advance frames as fast as the machine allows without waiting between them, SHALL keep the window responsive to input, and SHALL present the picture at least 30 times per second of wall-clock time rather than once per frame. (research 05 §6.3; the presentation rate is a port decision)

#### Scenario: Fast playback
- **WHEN** a recording of 5000 ticks is played with `--speed 0`
- **THEN** playback finishes well inside the 400 seconds that paced playback would take, and the window still reacts to the quit key

### Requirement: Title screen picture
The title screen SHALL show the title picture, the text `D I G G E R` at (100, 0) in colour 3, the two-line label of the selected game mode at y = 25 and y = 39 in colour 3, and the high-score table of that mode. Coordinates are in the game's 320×200 space. Sound effects and music from a previous game SHALL be stopped. (research 01 §2, research 03 §9.1–9.2)

#### Scenario: First start
- **WHEN** the program starts with default settings and no score file
- **THEN** the title screen shows `D I G G E R`, the label `ONE` at x = 220 over ` PLAYER ` at x = 192, and a table of ten `...` entries with score 0

### Requirement: Title animation
While the title screen is shown, the program SHALL play the cast animation in the right-hand panel as a cycle of 287 frames (frames 0 to 286) at the frame time, restarting at frame 0. Key points: a nobbin appears at (292, 63) on frame 50 and is labelled `NOBBIN` on frame 83; `HOBBIN` on frame 156; `DIGGER` on frame 163; `GOLD` on 183; `EMERALD` on 203; `BONUS` on 223; Digger shoots the hobbin on frames 232–238 and the nobbin on frames 251–260. Labels use colour 2. (research 03 §9.3)

#### Scenario: Labels appear on schedule
- **WHEN** the title screen has shown 164 frames since it appeared
- **THEN** the labels `NOBBIN` at (216, 64), `HOBBIN` at (216, 83) and `DIGGER` at (216, 102) are visible and `GOLD` is not

#### Scenario: Cycle restarts
- **WHEN** frame 286 of the animation has been shown
- **THEN** the next frame clears the cast panel and the cycle starts again at frame 0

### Requirement: Mode selection on the title screen
The mode-change action (default N) SHALL select the next game mode in the order: one player, two players, two players simultaneous, gauntlet, two-player gauntlet, then one player again. The mode label and the high-score table SHALL be updated at once. Labels are `ONE`/` PLAYER `, `TWO`/` PLAYERS`, `TWO PLAYER`/`SIMULTANEOUS`, `GAUNTLET`/`MODE`, `TWO PLAYER`/`GAUNTLET`. The title screen opens with the configured mode. (research 01 §2.1)

#### Scenario: Cycling once
- **WHEN** the title screen shows `ONE PLAYER` and the player presses N
- **THEN** the label reads `TWO` over ` PLAYERS` and no game starts

#### Scenario: Wrapping around
- **WHEN** the label reads `TWO PLAYER` over `GAUNTLET` and the player presses N
- **THEN** the label reads `ONE` over ` PLAYER ` and the one-digger normal table is shown

### Requirement: Starting a game from the title screen
On the title screen, pressing any key that is not a modifier key and is not bound to mode change, key redefinition, quit, save recording, sound toggle, music toggle, speed up, speed down or fullscreen SHALL start a game in the selected mode. So SHALL the fire or pause button of a gamepad. A key held down from before the title screen appeared SHALL NOT start a game. (research 05 §1.5 and §1.9; excluding the listed function keys is a port decision)

#### Scenario: Space starts
- **WHEN** the player presses Space on the title screen
- **THEN** a game starts in the selected mode with the level drawn and the main tune playing

#### Scenario: Music toggle does not start
- **WHEN** the player presses F7 on the title screen
- **THEN** music is switched off or on and the title screen stays

### Requirement: Other title screen actions
On the title screen the quit action (default F10) SHALL end the program; the redefine action (default K) SHALL open key redefinition and return to the title screen afterwards; the save-recording action (default F8) SHALL save the recording of the last game played in this run, once, and do nothing when there is none. (research 01 §2)

#### Scenario: Quit from the title
- **WHEN** the player presses F10 on the title screen
- **THEN** the program shuts down cleanly and exits with status 0

#### Scenario: Save without a game
- **WHEN** the player presses F8 on the title screen before any game was played
- **THEN** no file is written and the title screen stays

### Requirement: Pause
The pause action (default Space) during play SHALL freeze the game after the current tick: no ticks elapse, the gauntlet clock stands still, sound is silenced, and the top line shows `PRESS ANY KEY` at (80, 0) in colour 1 in place of scores and lives. The next key press or gamepad button press SHALL remove the text, restore scores and lives, resume sound and continue the game; that key press SHALL NOT also act as a game control. (research 01 §3.3)

#### Scenario: Pausing a gauntlet game
- **WHEN** the player pauses a gauntlet game showing 1:30 and presses a key 60 seconds later
- **THEN** the clock still shows 1:30 when play continues

#### Scenario: Resume key is consumed
- **WHEN** the game is paused and the player presses the player-1 fire key
- **THEN** the game continues and Digger does not fire because of that press

### Requirement: Losing focus
When the window loses keyboard focus, all keys and gamepad controls SHALL be treated as released. If a game played by a person is in progress, it SHALL pause as if the pause action had been used and stay paused until a key or button is pressed after focus returns. The title screen and playback of recordings SHALL keep running without focus. (port decision)

#### Scenario: Switching to another window mid-game
- **WHEN** the player holds the right key during a game and switches to another application
- **THEN** the game shows `PRESS ANY KEY` and Digger does not keep moving right after the player returns and resumes

#### Scenario: Playback keeps running
- **WHEN** a recording is being played back and the window loses focus
- **THEN** playback continues to its end

### Requirement: Quitting a game
The quit action during a game, a pause, a playback or the game-over sequence SHALL end it at once and return to the title screen, without high-score entry. A game ended this way SHALL still be saved to the record file when one was named on the command line. (research 01 §3.4, research 05 §4.5)

#### Scenario: Quit mid-game
- **WHEN** the player presses F10 during level 2
- **THEN** sound stops and the title screen appears without `GAME OVER` and without initials entry

### Requirement: Clean shutdown
Closing the window SHALL be honoured in every state, including pause, initials entry and key redefinition. On any exit the program SHALL stop the audio device, write a pending record file, write changed settings to the configuration file, and leave the score file complete. Exit status is 0 unless a verification run dictates another. (research 05 §1.1 documents upstream exiting without cleanup; clean shutdown is a port decision)

#### Scenario: Closing during a recorded game
- **WHEN** the program was started with `--record game.drf` and the window is closed during play
- **THEN** `game.drf` exists, holds the game up to that point with its end-of-game marker, and the exit status is 0

#### Scenario: Closing during initials entry
- **WHEN** the window is closed after the player typed one of three initials
- **THEN** the program exits with status 0, no new entry is added to the score table, and the score file is unchanged and readable

### Requirement: Audio device
At start the program SHALL open the default audio output for a mono signal at the configured sample rate (default 44100 Hz) and buffer size (default 512 frames, about 11.6 ms). On a device with several channels, every channel SHALL carry the same signal. If the device refuses the rate or buffer size, the program SHALL use the device's defaults and say so on standard error. The master volume setting (0 to 100) scales the output. (research 04 §4.2 and §6; fallback and volume are port decisions)

#### Scenario: Default device
- **WHEN** the program starts on a machine with a working sound card and default settings
- **THEN** the title screen is silent, and starting a game plays the main tune at 44100 Hz

#### Scenario: Stereo-only device
- **WHEN** the default output only accepts two channels
- **THEN** effects and music are heard equally from both channels

#### Scenario: Volume zero
- **WHEN** the master volume is 0
- **THEN** the device is open and the output is silence

### Requirement: Running without an audio device
If no audio output can be opened, or the device fails while running, the program SHALL print one notice on standard error, continue silently, and behave identically in every other respect. Triggering the sound toggle SHALL try to open the device again. (research 04 §5 and §6; retrying is a port decision)

#### Scenario: No sound card
- **WHEN** the program starts on a machine without any audio output
- **THEN** standard error carries a line starting with `digger: notice: no audio device`, and the game is fully playable in silence

#### Scenario: Device plugged in later
- **WHEN** the program runs silently for lack of a device, a device is then attached, and the player presses F9 twice
- **THEN** sound is heard after the press that switches sound on

### Requirement: Sound and music toggles
The sound toggle (default F9) SHALL switch all sound on and off; the music toggle (default F7) SHALL switch the background tunes on and off while effects, and the death tune, continue. Both work on the title screen and in a game. After a quiet start (`--quiet` or sound off in the file) the sound toggle SHALL switch sound on. (research 04 §3.4 and §6; re-enabling after a quiet start fixes an upstream bug)

#### Scenario: Re-enabling after --quiet
- **WHEN** the program is started with `--quiet`, a game is started and the player presses F9
- **THEN** effects and music are heard from then on

#### Scenario: Music off, effects on
- **WHEN** the player presses F7 during a game with sound on
- **THEN** the main tune stops and eating an emerald still makes its sound

### Requirement: Game-over sequence
When a player's game ends, the program SHALL run high-score entry for each digger of that player whose score qualifies, and then let the game show its own closing notice: `GAME OVER` at (104, 0), or `TIME UP` at (120, 0) in gauntlet mode, in colour 3 for 50 frames, whether or not initials were entered. Then the other player continues (alternating mode) or the title screen returns. (research 01 §6.3; the notice after an entry is a port decision of the `game-flow` capability)

#### Scenario: Ordinary game over
- **WHEN** a one-player game ends with a score that does not qualify
- **THEN** `GAME OVER` is shown for 50 frames (4 seconds at the default speed) and the title screen follows

#### Scenario: Gauntlet time-out with a qualifying score
- **WHEN** the gauntlet clock reaches zero and the score qualifies
- **THEN** initials entry follows, then `TIME UP` is shown for 50 frames, and `GAME OVER` is not shown

#### Scenario: First of two alternating players is out
- **WHEN** player 1 loses the last life in a two-player game while player 2 has lives left
- **THEN** player 1's game-over sequence runs and player 2's turn follows

### Requirement: Playback on screen
A recording given for playback SHALL be shown in the window with picture and sound at the current speed. Player movement and fire controls SHALL be ignored during playback; pause, speed, sound, music, fullscreen and quit actions work. When the recorded game ends, its closing notice is shown as in a live game and the program SHALL go on without high-score entry. (research 05 §3, research 01 §6.3)

#### Scenario: Watching a recording
- **WHEN** the program is started with `--play thevoid.drf`
- **THEN** the recorded game is shown from its first frame, pressing the arrow keys does not change Digger's path, and the title screen appears when the recording ends
