# Spec Delta

## Purpose

Keeps the ten best scores of each kind of game with the players' initials, shows them on the title screen, lets a player who earned a place enter initials, and stores the tables between runs.

## ADDED Requirements

### Requirement: Four score tables
The program SHALL keep four separate tables: normal game with one Digger, gauntlet with one Digger, normal game with two Diggers, and gauntlet with two Diggers. One-player and alternating two-player games share the normal one-Digger table; the two simultaneous modes use the two-Digger tables. (research 01 §6.4)

#### Scenario: Gauntlet scores stay apart
- **WHEN** a player enters a score of 50000 after a gauntlet game and then selects one-player mode on the title screen
- **THEN** the one-player table does not contain that entry and the gauntlet table does

#### Scenario: Alternating players share the one-player table
- **WHEN** player 2 of an alternating two-player game enters a qualifying score
- **THEN** the entry appears in the table shown for one-player mode

### Requirement: Table contents
Each table SHALL hold exactly ten entries ordered from highest to lowest, each made of three initials and a score from 0 to 999999. A table that was never written holds ten entries with initials `...` and score 0. (research 01 §6.3–6.4)

#### Scenario: Fresh table
- **WHEN** the program runs for the first time
- **THEN** all four tables consist of ten `...` entries with score 0

### Requirement: Eligible games
Only a game played by a person from start to end SHALL be considered for the tables: playback of a recording (`--play`, `--verify`) and games ended with the quit action or by closing the window never enter them. Games with unlimited lives are eligible, as upstream, because that setting only lifts the five-lives limit on bonus lives. (research 01 §2.1 and §6.3)

#### Scenario: Playback of a record-breaking game
- **WHEN** a recording that ends with 500000 points is played back with `--play`
- **THEN** no initials are asked for and the tables are unchanged

#### Scenario: Unlimited lives
- **WHEN** a game started with `--unlimited-lives` ends with a score above the tenth entry
- **THEN** initials entry is offered as in any other game

### Requirement: Qualifying score
A score SHALL qualify only if it is strictly greater than the score of the tenth entry of the table for the game's mode. The score compared and stored is the six-digit score shown on screen, which restarts from 0 after passing 999999. In games with two Diggers each Digger's score is considered separately, first Digger first. (research 01 §6.2–6.3)

#### Scenario: Equal to tenth place
- **WHEN** the tenth entry has 1000 points and a game ends with exactly 1000
- **THEN** the score does not qualify and `GAME OVER` is shown

#### Scenario: Zero against an empty table
- **WHEN** a game ends with 0 points and the table is all `...` with 0
- **THEN** the score does not qualify

#### Scenario: Both Diggers qualify
- **WHEN** a two-player simultaneous game ends with 9000 for the first Digger and 7000 for the second, both above tenth place
- **THEN** initials are asked for `PLAYER 1` and then for `PLAYER 2`, and both entries are in the two-Digger table

### Requirement: Insertion
A new entry SHALL be placed above every existing entry with a lower or equal score and below every entry with a higher score; the entries below it move down one place and the former tenth entry is dropped. (research 01 §6.3)

#### Scenario: Tie goes above
- **WHEN** the table's second entry is `AAA 5000` and a player enters `BBB` with 5000
- **THEN** `BBB 5000` is second and `AAA 5000` is third

#### Scenario: New best score
- **WHEN** a player enters a score higher than the first entry
- **THEN** the new entry is first and the previous tenth entry is gone

### Requirement: New high score screen
For a qualifying score the program SHALL clear the screen and show the scores on the top line, `PLAYER 1` or `PLAYER 2` at (108, 0) in colour 2, ` NEW HIGH SCORE ` at (64, 40) in colour 2, `ENTER YOUR` at (100, 70), ` INITIALS` at (100, 90) and `_ _ _` at (128, 130). All sound SHALL stop while this screen is shown. Coordinates are in the game's 320×200 space. (research 01 §6.3)

#### Scenario: Screen layout
- **WHEN** player 1's score qualifies
- **THEN** the screen shows `PLAYER 1`, ` NEW HIGH SCORE `, `ENTER YOUR`, ` INITIALS` and three underscores at x = 128, 152 and 176 on row y = 130, and no music plays

### Requirement: Initials entry
The player SHALL enter exactly three letters. Each accepted letter replaces the underscore at its position (x = 128, 152, 176; y = 130). Backspace or Delete returns to the previous position and shows its underscore again. After the third letter the screen stays for 20 more frames, then the entry is inserted, the table saved, and the title screen returns. The initials are also handed to the game's recording. (research 01 §6.3, research 05 §1.10)

#### Scenario: Three letters
- **WHEN** the player types `M`, `S`, `X`
- **THEN** the letters appear at x = 128, 152 and 176, and 20 frames later the title screen shows `MSX` with the score in the table

#### Scenario: Backspace at the first position
- **WHEN** the player presses Backspace before typing any letter
- **THEN** nothing changes and the first position still awaits a letter

#### Scenario: Initials in the recording
- **WHEN** a recorded game ends with the initials `ABC` entered
- **THEN** the saved classic-format recording carries the initials line `*ABC`

### Requirement: Flashing during entry
While initials are being entered and during the 20 closing frames, the picture SHALL alternate between the normal palette and the alternate palette once per frame, ending on the normal palette. With the `reduce_flashing` setting on, the picture SHALL stay in the normal palette. (research 01 §6.3 for the flashing; one change per frame and the setting are port decisions)

#### Scenario: Flashing at the default speed
- **WHEN** the initials screen is shown at the default speed
- **THEN** the text colours switch palette 12.5 times per second

#### Scenario: Flashing switched off
- **WHEN** `video.reduce_flashing = true` and the initials screen is shown
- **THEN** the colours do not change from frame to frame

### Requirement: Table on the title screen
The title screen SHALL show the table of the selected mode: `HIGH SCORES` at (16, 25) in colour 3 and entry i (1 to 10) at (16, 31 + 13 × i) as the three initials, two spaces and the score right-aligned in six characters padded with spaces. The first entry is in colour 2, the others in colour 1. (research 01 §6.3)

#### Scenario: Row text and position
- **WHEN** the third entry of the selected table is `ABC` with 12345 points
- **THEN** the title screen shows `ABC   12345` at (16, 70) in colour 1

#### Scenario: Default entry
- **WHEN** an entry has never been filled
- **THEN** its row reads `...       0`

### Requirement: Score file
All tables SHALL be stored in one file, `scores.toml`, in the per-user data directory: `$XDG_DATA_HOME/digger/` (or `~/.local/share/digger/`) on Linux, `%APPDATA%\digger\data\` on Windows, `~/Library/Application Support/digger/` on macOS; `--data-dir <dir>` names another directory. The file is in the port's own text format and starts with a format version number, currently 1. (port decision; research 01 §6.4 describes the upstream format, which is not used)

#### Scenario: File appears after the first entry
- **WHEN** a player enters initials for the first time on Linux with default directories
- **THEN** `~/.local/share/digger/scores.toml` exists, declares format 1 and contains that entry

#### Scenario: Scores survive a restart
- **WHEN** the program is closed and started again
- **THEN** the title screen shows the same tables as before

### Requirement: Safe saving
The score file SHALL be written whenever an entry is inserted, before the title screen returns, in such a way that an interrupted write leaves either the old or the new complete file. If the file cannot be written, the program SHALL print a notice naming the file and cause on standard error, keep the entry in memory for the rest of the run, and continue. (port decision)

#### Scenario: Read-only data directory
- **WHEN** the data directory is not writable and a player enters initials
- **THEN** a notice is printed, the title screen shows the new entry, and the game stays playable

### Requirement: Missing, damaged or newer score file
A missing score file SHALL be treated as default tables. A file that cannot be parsed, has a wrong structure, or declares a newer format version SHALL also give default tables, with one notice on standard error; before the next save it SHALL be kept by renaming it to `scores.toml.bad`. A table absent from a valid file is a default table. The score file SHALL never stop the program from starting. (research 01 §6.4 for defaulting on an invalid file; keeping the bad file is a port decision)

#### Scenario: Garbage in the file
- **WHEN** `scores.toml` contains random bytes
- **THEN** the program starts, prints a notice naming the file, and shows default tables

#### Scenario: Damaged file is preserved
- **WHEN** the file was unreadable at start and a player then enters a score
- **THEN** the old content is found in `scores.toml.bad` and `scores.toml` holds the new entry

#### Scenario: File from a newer version
- **WHEN** the file declares format 2
- **THEN** default tables are shown and the file is not overwritten unless a new score is entered, in which case it is first renamed to `scores.toml.bad`

### Requirement: Tables for custom level files
Games played with a level file SHALL use their own set of four tables, kept in the same score file and identified by the level file's name without directory, so that scores from different level sets are never mixed with each other or with the built-in levels. (research 01 §6.4 documents upstream storing scores inside the level file; this form is a port decision)

#### Scenario: Custom levels do not touch the main tables
- **WHEN** a player enters a score in a game started with `--levels hard.dlf`
- **THEN** that entry is shown only when the program runs with `--levels hard.dlf`, and the built-in level tables are unchanged
