# Spec Delta

## Purpose

Defines how the game behaves when it is played in a web browser from a static
page: how it is delivered, shown, controlled, heard and paced, where it keeps
settings, scores and recordings, and which browsers and download sizes are
supported. Game rules are the same as in the desktop build and are specified
elsewhere.

## ADDED Requirements

### Requirement: Static page delivery
The web build SHALL be a fixed set of files that any static file server can
serve over HTTP or HTTPS, with no server-side code. All references between
the files SHALL be relative, so the game works at a site root and in a
sub-directory. After the page has loaded, the game SHALL make no further
network requests. (port decision; research 05 §8.4)

#### Scenario: Served from a sub-directory
- **WHEN** the files are served at `https://example.org/games/digger/` and the page is opened
- **THEN** the title screen appears and no request leaves the `/games/digger/` path

#### Scenario: No network use during play
- **WHEN** the title screen has appeared and the network connection is then cut
- **THEN** a complete game can be played, a score saved and a recording downloaded

### Requirement: Loading and failure states
The page SHALL show a loading indicator from first paint until the first game
frame is drawn. If the browser lacks a required feature, a file fails to load
or the game stops with an internal error, the page SHALL replace the game
area with a readable message that names the problem. It MUST NOT be left
blank or frozen without explanation. (port decision; research 05 §8.4)

#### Scenario: Normal load
- **WHEN** the page is opened in a supported browser
- **THEN** a loading indicator is visible until the title picture is drawn, and is then removed

#### Scenario: Game file missing
- **WHEN** the page is opened and the request for the game's program file fails
- **THEN** the game area shows a message saying the game could not be loaded, and no canvas is shown

#### Scenario: Scripting disabled
- **WHEN** the page is opened with scripting disabled
- **THEN** the page shows a sentence saying the game needs scripting to run

### Requirement: Canvas presentation and scaling
The web build SHALL show the 640×400 picture on one canvas by the desktop
`integer` scaling rule, applied to the browser viewport in device pixels: the largest
whole-number scale that fits, at least 1, centred on black, with no smoothing
between pixels. A viewport smaller than 640×400 SHALL show the whole picture
reduced to fit with its 8:5 shape kept. The scale SHALL be recomputed when
viewport size or pixel density changes. (port decision; research 07 §A.7)

#### Scenario: Full-HD viewport
- **WHEN** the viewport is 1920×1080 device pixels
- **THEN** the picture is drawn at scale 2 as 1280×800 device pixels, centred, with black bars of 320 pixels left and right and 140 pixels above and below

#### Scenario: High-density display
- **WHEN** the viewport is 1280×720 CSS pixels on a display with 2 device pixels per CSS pixel
- **THEN** the picture is drawn at scale 3 as 1920×1200 device pixels with every game pixel a sharp 3×3 block

#### Scenario: Window resized during play
- **WHEN** the viewport shrinks from 1920×1080 to 1000×700 device pixels during a game
- **THEN** from the next displayed frame the picture is at scale 1 (640×400), centred, and the game state is unaffected

### Requirement: Fullscreen
The page SHALL offer a fullscreen button, and the desktop build's fullscreen
key SHALL work in the browser. In fullscreen the scaling rule SHALL apply to
the whole screen. Leaving fullscreen by the browser's own means (for example
the Escape key) SHALL return the game to the page layout without pausing or
restarting it. (research 05 §8.4)

#### Scenario: Entering fullscreen from the button
- **WHEN** the player activates the fullscreen button on a 2560×1440 screen
- **THEN** the game fills the screen at scale 3 (1920×1200), centred on black

#### Scenario: Leaving fullscreen with the browser
- **WHEN** the game is in fullscreen and the player presses Escape
- **THEN** the page layout returns, the game continues from the same frame and no game key press is registered for Escape

### Requirement: Keyboard input
The web build SHALL use the same default key bindings, identified by physical
key position, and the same key-redefinition procedure as the desktop build.
While the game has keyboard focus the page SHALL suppress the browser's own
action for every key bound to a game function (scrolling on arrows and Space,
help on F1, focus change on Tab, menu on F10). When the game loses keyboard
focus, all held game keys SHALL be treated as released. (port decision;
research 07 §A.7)

#### Scenario: Arrow keys do not scroll the page
- **WHEN** the page is taller than the viewport and the player holds the down arrow during a game
- **THEN** Digger moves down and the page does not scroll

#### Scenario: Fire key does not open browser help
- **WHEN** the player presses F1 during a game
- **THEN** Digger fires and no help page or tab opens

#### Scenario: Focus lost with a key held
- **WHEN** the player holds the right arrow and clicks the browser's address bar
- **THEN** Digger stops receiving the move-right input as if the key had been released

### Requirement: Controls panel
The page SHALL show, outside the game picture, a panel listing every game
function with the key currently bound to it. The panel SHALL be updated
within 1 second of the player finishing a key redefinition. (research 05
§8.4)

#### Scenario: Default bindings shown
- **WHEN** the page is opened with no stored settings
- **THEN** the panel lists the default keys, including the arrow keys for movement, F1 for fire and Space for pause

#### Scenario: Binding changed
- **WHEN** the player redefines the fire key to the Enter key
- **THEN** the panel shows Enter for fire without a page reload

### Requirement: Gamepad input
The web build SHALL accept gamepads with the same mapping as the desktop
build. A gamepad SHALL become usable from its first button press after page
load, without a reload, and one connected during play SHALL work from its
first button press. When a gamepad is disconnected, the controls it was
holding SHALL be treated as released. (port decision)

#### Scenario: Gamepad connected before the page was opened
- **WHEN** a gamepad is already plugged in, the page is opened and the player presses a button on the gamepad
- **THEN** that press and all later gamepad input act as they do in the desktop build

#### Scenario: Gamepad unplugged while moving
- **WHEN** the player holds left on a gamepad and the gamepad is unplugged
- **THEN** Digger stops receiving the move-left input and keyboard control keeps working

### Requirement: Audio starts only after a user gesture
The web build SHALL NOT attempt to produce sound before the player's first
key press, pointer press or gamepad button press on the page. Until then the
game SHALL run silently and the page SHALL show a visible prompt, outside the
game picture's pixels, saying that a key press or click turns sound on. The
first such gesture SHALL start audio, remove the prompt and still be handled
as normal game input. (port decision)

#### Scenario: Page opened and left alone
- **WHEN** the page is opened and no key, pointer button or gamepad button is pressed for 60 seconds
- **THEN** the title screen runs, no sound is produced, the browser reports no blocked-autoplay error and the prompt stays visible

#### Scenario: First key press
- **WHEN** the prompt is visible and the player presses the key that starts a game
- **THEN** the prompt disappears, the game starts from that same key press and its sounds are audible

#### Scenario: Prompt is not part of the picture
- **WHEN** the prompt is visible during playback of a recording
- **THEN** the playback reaches the same final score and state checkpoints as without the prompt

### Requirement: Sound continuity and audio failure
Sound state SHALL advance while audio is locked, so that music already in
progress when audio starts continues from its current position and effects
that ended earlier are not replayed. If the browser refuses or fails to
provide audio output, the game SHALL continue silently and the prompt SHALL
be replaced by a notice that sound is unavailable. (port decision)

#### Scenario: Audio unlocked during music
- **WHEN** background music has been playing silently for 10 seconds and the player then presses a key
- **THEN** the music becomes audible at the point 10 seconds into the tune, not from its start

#### Scenario: No audio device
- **WHEN** the player presses a key and the browser cannot open an audio output
- **THEN** the game keeps running without sound and the page shows that sound is unavailable

### Requirement: Frame pacing from the animation callback
The web build SHALL advance the game only from the browser's animation
callback, running one game frame each time one frame length of real time has
accumulated. The frame length SHALL be the desktop value: 80,000 microseconds
(12.5 frames per second) at default speed. Game speed SHALL NOT depend on the
display refresh rate. As on the desktop, at most 3 game frames SHALL run in
one callback; any further backlog SHALL be discarded. (research 05 §6.3)

#### Scenario: 60 Hz display
- **WHEN** the game runs at default speed for 60 seconds on a 60 Hz display
- **THEN** between 745 and 755 game frames have run

#### Scenario: 144 Hz display
- **WHEN** the game runs at default speed for 60 seconds on a 144 Hz display
- **THEN** between 745 and 755 game frames have run, the same as on a 60 Hz display

#### Scenario: Long stall
- **WHEN** the browser delivers no animation callback for 2 seconds while the page stays visible and then resumes
- **THEN** at most 3 game frames run in the first callback and the game then continues at normal speed without fast-forwarding

### Requirement: Hidden tab pauses the game
When the page becomes hidden (tab switched, window minimised), the web build
SHALL stop advancing the game and silence audio within 100 milliseconds.
During live play it SHALL enter the same paused state as the pause key and
stay paused until the player resumes as on the desktop. On the title screen
and during playback of a recording it SHALL resume by itself when the page is
visible again. Hidden time SHALL never be caught up. (port decision; research
05 §1.9)

#### Scenario: Tab switched during a game
- **WHEN** the player switches to another tab for 30 seconds during a game and then returns
- **THEN** the game shows its pause state at the frame where it was hidden, no monster has moved, and play continues only after the player resumes

#### Scenario: Gauntlet clock while hidden
- **WHEN** a gauntlet game with 90 seconds left is hidden for 30 seconds
- **THEN** 90 seconds are still left when the player resumes

#### Scenario: Playback while hidden
- **WHEN** a recording is playing back and the tab is hidden for 10 seconds and shown again
- **THEN** playback continues by itself from the frame where it stopped and ends with the recorded score

### Requirement: Settings and high scores persist in local storage
The web build SHALL keep the desktop build's settings under the browser
local-storage key `digger:settings` and its high-score tables, gauntlet
separate from normal, under `digger:scores`. It SHALL write each as soon as
it changes and read both at start, and SHALL use no key that does not start
with `digger:`. Missing or unreadable stored data SHALL give defaults without
stopping the game. (research 05 §2.1, §8.4; research 07 §A.7)

#### Scenario: Setting survives a reload
- **WHEN** the player switches music off, then reloads the page
- **THEN** music is off after the reload

#### Scenario: New high score survives closing the tab
- **WHEN** the player enters initials for a score of 8,650, closes the tab immediately and opens the page again
- **THEN** the high-score table shows that entry with 8,650, and a gauntlet score set earlier is still listed only in the gauntlet table

#### Scenario: Damaged stored settings
- **WHEN** the value stored under `digger:settings` is not valid settings data and the page is opened
- **THEN** the game starts with default settings, stored high scores are still shown, and the next settings change replaces the damaged value

### Requirement: Play without usable storage
If local storage is unavailable, refuses a write or is full, the web build
SHALL keep running with settings and scores held in memory for the life of
the page, and SHALL show one notice saying that settings and scores will not
be kept. A storage failure MUST NOT interrupt a game in progress. (port
decision)

#### Scenario: Storage blocked by the browser
- **WHEN** the page is opened in a browser configured to deny site storage
- **THEN** the game is fully playable, a notice says settings and scores will not be kept, and a high score entered stays in the table until the page is closed

#### Scenario: Write fails during play
- **WHEN** a settings write fails during a game because storage is full
- **THEN** the game continues without a pause and the notice appears once

### Requirement: Saving a recording as a download
Where the desktop build writes a recording file, the web build SHALL hand the
recording to the browser as a file download with the name the desktop build
would use and byte-identical content for the same game. The page SHALL also
offer a save-recording button that is enabled only when a finished game's
recording is available. The recording SHALL NOT be sent anywhere else.
(research 07 §A.6)

#### Scenario: Saving from the title screen
- **WHEN** a game has ended and the player presses the save-recording key (F8 by default) on the title screen
- **THEN** the browser downloads one recording file whose content replays to the same final score on the desktop build

#### Scenario: Nothing to save
- **WHEN** the page has just been opened and no game has been played
- **THEN** the save-recording button is disabled and the save-recording key causes no download

### Requirement: Loading a recording through a file picker
The page SHALL offer a play-recording button that opens the browser's file
picker for `.drf` and `.edrf` files. The chosen file SHALL be read locally
and played back with the desktop playback rules; when playback ends the game
SHALL return to the title screen. A file that is malformed, of an unsupported
kind or larger than 16 MiB SHALL be rejected with a message naming the
reason, leaving the title screen running. (port decision; research 07 §A.6)

#### Scenario: Upstream recording plays to its recorded result
- **WHEN** the player picks one of the upstream `.drf` recordings from the conformance suite
- **THEN** playback ends with the same final score, level and tick count as the desktop build reports for that file, and the title screen returns

#### Scenario: Extended recording diverges
- **WHEN** the player picks an `.edrf` file whose state checkpoint does not match during playback
- **THEN** playback stops and the page reports a divergence with the tick at which it was detected

#### Scenario: Not a recording
- **WHEN** the player picks a 20 MiB video file or a text file that does not begin with a recording header
- **THEN** a message says why the file was rejected and the title screen keeps running

### Requirement: Start-up options from URL query parameters
In place of a command line the web build SHALL read the query parameters
`speed`, `level` (both as the desktop options), `mode` (the desktop mode
names, `one-player` to `two-digger-gauntlet`) and `quiet` (present, or `1`:
start with sound off). They SHALL apply to that page load only and SHALL NOT
be stored. A value outside the allowed set SHALL be ignored with a visible
warning naming parameter and value. Unknown parameters SHALL be ignored.
(port decision; research 07 §A.6)

#### Scenario: Gauntlet at a chosen speed
- **WHEN** the page is opened with `?mode=gauntlet&speed=30`
- **THEN** starting a game gives a gauntlet game of 120 seconds at speed 30, and after a reload without parameters the stored mode and speed apply again

#### Scenario: Start level and quiet start
- **WHEN** the page is opened with `?level=5&quiet` and a game is started
- **THEN** the game begins on level 5, no sound prompt is shown, and pressing the sound toggle key turns sound on

#### Scenario: Invalid values
- **WHEN** the page is opened with `?speed=fast&mode=two-players&colour=red`
- **THEN** a warning names `speed` and `fast`, the stored or default speed is used, the two-players mode is applied, and `colour` causes no warning

### Requirement: No command line and no exit
The web build SHALL have no command line and SHALL never close or navigate
away from its page. The action that exits the desktop program from the title
screen SHALL leave the title screen running in the browser. Recording and
playback SHALL be reachable only through the page controls and keys described
in this capability. (research 05 §8.4)

#### Scenario: Exit key on the title screen
- **WHEN** the player presses the exit key (F10 by default) on the title screen
- **THEN** the title screen keeps running and the page stays open

#### Scenario: Back to title during a game
- **WHEN** the player presses the return-to-title key during a game
- **THEN** the game ends and the title screen is shown, as on the desktop

### Requirement: Download size budget
The files a browser must fetch to reach the title screen SHALL total no more
than 2 MiB (2,097,152 bytes) uncompressed and no more than 1 MiB (1,048,576
bytes) when each is compressed with gzip at its highest level. Art, level and
sound data SHALL be inside these files; nothing is fetched later. A release
build that exceeds either limit SHALL fail. (port decision)

#### Scenario: Build within budget
- **WHEN** the web build is produced and its page, script and program files total 1.6 MiB uncompressed and 0.7 MiB compressed
- **THEN** the build succeeds and reports both totals

#### Scenario: Build over budget
- **WHEN** a change raises the uncompressed total to 2.2 MiB
- **THEN** the web build fails with a message giving the measured total and the 2 MiB limit

### Requirement: Supported browsers
The web build SHALL work in the two most recent stable major versions, at the
time of each release, of Chrome, Edge, Firefox and Safari on desktop
operating systems. A keyboard or a gamepad is required: when the device
reports touch as its only input, the page SHALL say so and SHALL still load
the game. Each release SHALL pass an automated check in the Chromium, Firefox
and WebKit browser engines. (port decision)

#### Scenario: Automated check on three engines
- **WHEN** a release is prepared
- **THEN** in each of Chromium, Firefox and WebKit the page loads, the title picture is drawn, a game starts from a key press and a setting survives a reload

#### Scenario: Touch-only device
- **WHEN** the page is opened on a phone with no keyboard or gamepad
- **THEN** the page says a keyboard or gamepad is needed and the title screen still appears

### Requirement: Version, licence and source shown on the page
The page SHALL show, outside the game picture, the game's version (the same
version string the desktop build reports), the licence name GNU GPL version 2
or later, the credits from the project's notice file or a link to them, and a
link to the source code of that exact version. (port decision; research 05
§8.4; research 07 §A.5)

#### Scenario: Footer of a released build
- **WHEN** the page of release 1.2.0 is opened
- **THEN** the footer shows version 1.2.0, names the GPL version 2 or later, and links to the source at the 1.2.0 tag
