## MODIFIED Requirements

### Requirement: Quit
A quit request on any frame SHALL end the game on that frame. The game reports that it finished because it was quit, with the scores, level and tick count reached; no game-over notice is shown. (research 01 §3.4)

#### Scenario: Quit during play
- **WHEN** a quit request arrives at tick count 300
- **THEN** the game finishes with reason "quit" and reports 300 ticks

## ADDED Requirements

### Requirement: Quit before the wind-down
A quit during the main phase or during its pause SHALL run no further tick but SHALL first do what the start of the wind-down does: the diggers' sprites are removed, the diggers become invisible and the music stops. The round end then follows as after a wind-down, so the round-end state the recordings check holds the diggers invisible. A quit during the wind-down SHALL go straight to the round end. (research 01 §3.2, §3.4; recordings `2riptest01.edrf` tick 1024 and `2test03.edrf` tick 2391)

#### Scenario: Quit hides the diggers before the round end
- **WHEN** a quit request arrives during the main phase of a two-digger game, or while it is paused
- **THEN** no further tick runs, the music-off and stop-all cues are emitted in that order, and at the round end both diggers are invisible and their sprites removed

#### Scenario: Quit during the wind-down
- **WHEN** a quit request arrives during the wind-down
- **THEN** no further wind-down frame runs and the round end follows at once
