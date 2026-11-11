# Spec Delta

## Purpose

Game flow is the frame-by-frame course of a game: which mode is played, how rounds and levels begin and end, in what order things are updated, how random numbers are produced, and what makes a game exactly reproducible from its configuration, seeds and inputs.

## ADDED Requirements

### Requirement: Game modes
The game SHALL support five modes: one player; two players alternating (one digger each, taking turns, each with their own level, field, score and lives); two diggers simultaneously (one shared field and level, separate scores and lives); gauntlet (one digger against a clock); and gauntlet with two simultaneous diggers. (research 01 §2.1)

#### Scenario: Alternating players keep separate progress
- **WHEN** player 1 of an alternating game is on level 3 and player 2 on level 1
- **THEN** each turn uses that player's own level number, field, score and lives

#### Scenario: Simultaneous diggers share the field
- **WHEN** digger 1 digs a tunnel in a two-digger game
- **THEN** digger 2 can walk through it on the same field

### Requirement: Invalid configurations are rejected
Configuring a game SHALL fail with an error, and no game SHALL start, when: two alternating players are combined with two simultaneous diggers; two alternating players are combined with gauntlet; or the start level is outside 1..1000. A gauntlet duration of 0 seconds SHALL be read as 120 and a duration above 3599 as 3599. (research 01 §1.1, §1.2, §9)

#### Scenario: Two players with two diggers
- **WHEN** a configuration asks for two alternating players and two simultaneous diggers
- **THEN** it is rejected with an error naming the conflicting settings

#### Scenario: Start level zero
- **WHEN** a configuration asks for start level 0
- **THEN** it is rejected with an error stating the valid range 1..1000

#### Scenario: Gauntlet duration limits
- **WHEN** gauntlet durations of 0 and 5000 seconds are configured
- **THEN** the games run with 120 and 3599 seconds

### Requirement: Game start
A game SHALL start with tick count 0, round index 0, all scores 0, 3 lives for every digger in play, every player's level set to the start level, and player 1's turn. Level state (earth, emeralds, bags) SHALL be created for both player positions before play begins. (research 01 §3.1)

#### Scenario: One-player start at level 5
- **WHEN** a one-player game is configured with start level 5
- **THEN** play begins on level 5 with layout 5, score 0 and 3 lives

### Requirement: Second player's level state in other modes
In every mode other than two players alternating, the second player position SHALL be initialised as level 0: an all-earth field with no emeralds and no bags. It SHALL be initialised this way at every game start, regardless of earlier games. This state takes no part in play but is part of the simulation state. (research 01 §3.1, §10 item 18)

#### Scenario: One-player game after a two-player game
- **WHEN** a one-player game is started after a two-player game was played
- **THEN** the second player position holds level 0 state exactly as in a freshly started program

### Requirement: Rounds
Play SHALL be divided into rounds; a round is one attempt, ending with the death of all diggers in play, the completion of the level, the gauntlet time-out or a quit. Rounds are numbered from 0 across the whole game. Each round consists of a start, a main phase of ticks, a wind-down of ticks and an end. (research 01 §3.2)

#### Scenario: Death then completion
- **WHEN** a player dies once on level 1 and then completes it
- **THEN** the death ends round 0, the completion ends round 1, and level 2 starts with round 2

### Requirement: Round start
A round after a new level or a change of player SHALL begin with a full playfield draw followed by placing diggers and resetting monsters. A round after a death on the same level and turn SHALL only reset sprites, diggers and monsters, leaving the field as it is. In both cases the round then refreshes scores and the lives display, requests the main music and clears control state before its first tick. (research 01 §3.2)

#### Scenario: Round after a death
- **WHEN** a one-player round ends in death with lives left
- **THEN** the next round starts with the same tunnels, emeralds and surviving bags, the digger at its start cell and no monsters on screen

### Requirement: Player announcement
In an alternating two-player game, the first round and every round following a change of player SHALL, after the playfield draw, show `PLAYER 1` or `PLAYER 2` at (108,0) for 45 frames, the text and that player's score cycling through text colours 1, 2, 3 once every three frames. No tick elapses. The text is then erased. (research 01 §3.2, research 03 §8.5)

#### Scenario: Turn passes
- **WHEN** player 1 dies and player 2 has lives
- **THEN** player 2's field is drawn, `PLAYER 2` flashes for 45 frames with the tick count unchanged, and play then starts

#### Scenario: One-player game
- **WHEN** a one-player round starts
- **THEN** no player announcement is shown

### Requirement: Update order of a main-phase tick
Each main-phase tick SHALL, in this order: reset the penalty counter; latch the fire controls read on the previous tick; update diggers (gauntlet clock, then for each digger in order: read controls, fireball, movement or death, emerald window; then bonus-mode timers); update monsters (spawning and bonus appearance, then slots 0..5); update bags 0..6; apply the penalty overflow; evaluate level completion; and increment the tick count. (research 01 §3.2, research 02 §6)

#### Scenario: Digger moves before monsters
- **WHEN** a digger and a monster would both move into the same empty spot on one tick
- **THEN** the digger's move is evaluated first and the monster's move sees the digger at its new position

#### Scenario: Bags see this tick's digging
- **WHEN** a digger removes the first slice of the cell under a resting bag
- **THEN** the bag starts wobbling in the bag update of the same tick

### Requirement: Level completion test
At the end of every main-phase tick the level SHALL be marked complete if at least one digger is alive and either no emeralds remain or no monsters remain (none on screen and none left to spawn). The main phase ends after the tick on which the level is marked complete or all diggers are dead. (research 01 §3.2)

#### Scenario: Last emerald
- **WHEN** a living digger eats the last emerald on tick N
- **THEN** tick N is the last main-phase tick of the round

#### Scenario: All monsters killed
- **WHEN** the last remaining monster of a round is killed while emeralds remain and a digger is alive
- **THEN** the level is marked complete

### Requirement: Wind-down
After the main phase the diggers' sprites SHALL be removed, the music stopped, and wind-down ticks run in the order bags, diggers, monsters. Diggers neither move nor consume movement samples; everything else, including random draws and the tick count, continues. The wind-down lasts at least 1 tick and ends after the first tick at which no bag is still moving and either that tick's penalty counter was below 8 or 20 wind-down ticks have run. (research 01 §3.2, research 02 §6)

#### Scenario: Quiet field
- **WHEN** a round's main phase ends with no bag moving and little activity
- **THEN** exactly one wind-down tick runs

#### Scenario: Busy field without moving bags
- **WHEN** every wind-down tick has a penalty counter of 8 or more and no bag is moving
- **THEN** exactly 20 wind-down ticks run

#### Scenario: Monsters keep drawing random numbers
- **WHEN** a living monster with no delay is on screen during a wind-down tick
- **THEN** it makes its move and its random draws as in the main phase

### Requirement: Round end
At the end of the wind-down, in this order: all sound cues SHALL be stopped; every fireball removed; the bonus removed; bags tidied and saved; the field saved for the player; and all monsters removed. The round's outcome is then decided: level completion, death, game over or time-out. (research 01 §3.2)

#### Scenario: Flying fireball at round end
- **WHEN** a round ends while a fireball is in flight
- **THEN** the fireball is gone and the digger's fireball is ready at the start of the next round

### Requirement: Level completion outcome
If at round end no emeralds remain or the level was marked complete, each digger in play that has lives but is not alive SHALL lose a life, the player's level SHALL rise by 1 (never above 1000) and the new level's state SHALL be created; the next round starts with a full draw. This applies even when the last emerald went on the tick the digger died. A completed level does not pass the turn to the other player. (research 01 §3.2, §10 item 11)

#### Scenario: Normal completion
- **WHEN** a player completes level 2 alive
- **THEN** no life is lost and the next round draws level 3

#### Scenario: Dying with the last emerald gone
- **WHEN** the last emerald is destroyed on the same tick the only digger is killed, and the round ends with 3 lives held
- **THEN** the player has 2 lives and the next round starts on the next level

### Requirement: Level-complete jingle and pause
When a round ends with the level marked complete, the level-done cue SHALL be emitted and play SHALL pause for 40 frames, during which no tick elapses, before the next level is drawn. No pause occurs when the level advances only because no emeralds remain after a death. (research 01 §3.2, research 04 §2, §5)

#### Scenario: Pause length
- **WHEN** a level is completed by a living digger
- **THEN** 40 frames pass between the last wind-down tick and the first frame of the next level's draw, with the tick count unchanged

### Requirement: Death outcome and passing the turn
If a round ends with all diggers dead and the level not complete, each digger in play with lives left SHALL lose one (never in gauntlet mode). In an alternating two-player game the turn then passes to the other player if that player has lives, with a full draw and a player announcement; otherwise the same player continues. (research 01 §3.2)

#### Scenario: Other player out of lives
- **WHEN** player 1 dies with lives remaining and player 2 has no lives
- **THEN** player 1 plays the next round without a redraw or announcement

### Requirement: Game over
When a round ends with all diggers dead, no lives left among the current player's diggers, and the mode is not gauntlet, that player's game is over: a 0-point scoring call SHALL be made for each of their diggers, the frame's outcome SHALL report the player and final score, and `GAME OVER` SHALL be shown at (104,0) in text colour 3 for 50 frames without ticks. The whole game finishes when the current player has no lives and the turn cannot pass. (research 01 §3.2, §6.3)

#### Scenario: One-player game over
- **WHEN** the digger of a one-player game dies with 1 life
- **THEN** after the wind-down `GAME OVER` is shown for 50 frames and the game then reports that it has finished

#### Scenario: Alternating game continues
- **WHEN** player 1 loses their last life while player 2 has lives
- **THEN** player 1's final score is reported, `GAME OVER` is shown for 50 frames, and player 2's turn follows

### Requirement: Gauntlet clock
The gauntlet clock SHALL start at duration × 1,193,181 units as an unsigned 32-bit value. At the start of each tick's digger update, in main phase and wind-down alike, the clock display is redrawn, the game is marked timed out if the clock is below 80,000, and 80,000 is subtracted with wrap-around. The clock therefore counts ticks, not real time, and stops whenever ticks stop. (research 01 §7.4)

#### Scenario: Two-minute gauntlet
- **WHEN** a 120-second gauntlet game runs without bonus time
- **THEN** it is marked timed out during tick 1790, after which the clock value is 4,294,949,016

#### Scenario: Clock runs while dying
- **WHEN** the digger of a gauntlet game is in its death sequence
- **THEN** the clock drops by 80,000 on every tick

### Requirement: Gauntlet time-out
The tick on which the game is marked timed out SHALL be completed in full and SHALL be the game's last tick: no further main-phase or wind-down tick runs. The round-end steps are performed, a 0-point scoring call is made for each digger, the outcome reports the final scores, the top line is cleared and `TIME UP` is shown at (120,0) in text colour 3 for 50 frames, and the game finishes. (research 01 §3.2, §6.3, §7.4)

#### Scenario: Time-out during the main phase
- **WHEN** the clock runs out on a main-phase tick
- **THEN** the tick count stops at that tick, `TIME UP` is shown for 50 frames and the game reports it has finished because time ran out

### Requirement: Frames and ticks
The game SHALL advance exactly one frame per step request. A frame is a tick frame when the simulation is updated (main phase or wind-down) and otherwise a waiting frame (level draw, player announcement, level-complete pause, game-over or time-up notice, pause). The tick count SHALL increase by exactly 1 on every tick frame and never on a waiting frame. Every frame SHALL report which kind it was. (research 01 §4, research 05 §6.3)

#### Scenario: Tick count across a level change
- **WHEN** a level is completed at tick count 900 and the next level's first tick runs
- **THEN** the tick count after that tick is 901 plus the number of wind-down ticks, however many waiting frames passed in between

### Requirement: Per-frame input
Each step request SHALL carry, for each of the two control slots, five on/off controls (up, down, left, right, fire), each on if it was held at the frame's sampling moment or pressed at any time since the previous frame, together with a pause request and a quit request. Controls are read on tick frames only and ignored on waiting frames. (research 05 §1.6, §1.7, §6.3)

#### Scenario: Input during the level draw
- **WHEN** a direction is reported on frames of the level draw only
- **THEN** it has no effect on the digger

### Requirement: Speed setting and tick length
The intended real-time length of a frame SHALL be speed × 2000 microseconds, with a default speed of 40 (80,000 microseconds, 12.5 frames per second) and speed 0 meaning "do not wait". A "faster" step SHALL subtract 10,000 microseconds only while the length exceeds 10,000; a "slower" step SHALL add 10,000. The speed SHALL have no influence on any game result. (research 01 §4, research 05 §6.3)

#### Scenario: Default speed
- **WHEN** no speed is configured
- **THEN** the frame length is 80,000 microseconds

#### Scenario: Faster at the floor
- **WHEN** the frame length is 10,000 microseconds and "faster" is requested
- **THEN** the frame length stays 10,000

#### Scenario: Same game at two speeds
- **WHEN** the same configuration, seeds and inputs are run at speed 40 and at speed 0
- **THEN** every frame's picture, sound commands and state are identical

### Requirement: Determinism
Given the same configuration, the same round seeds and the same per-frame inputs, the game SHALL produce the same picture, the same sound commands in the same frames and the same simulation state after every frame, on every platform, whether or not anything is displayed or played. No game result SHALL depend on real time, on the audio system or on how fast frames are requested. (research 01 §7.3, research 04 §5, research 05 §6.3)

#### Scenario: Repeated run
- **WHEN** a game is run twice with identical configuration, seeds and inputs
- **THEN** the pictures after every frame are identical pixel for pixel and the final scores, level and tick count are equal

#### Scenario: Headless run
- **WHEN** the same game is run once with a display and sound and once with neither
- **THEN** the simulation states after every frame are equal

### Requirement: Random-number generator
The game SHALL use one generator with a signed 32-bit state. A draw with modulus n SHALL set state = state × 22,695,477 + 1 with 32-bit wrap-around and return (state with its top bit cleared) mod n. The modulus is always at least 5. Sound SHALL never draw from this generator. (research 01 §7.1, research 02 §8)

#### Scenario: Sequence from state 0
- **WHEN** the state is 0 and three numbers are drawn with modulus 14
- **THEN** the states are 0x00000001, 0x015A4E36 and 0x8082A52F and the results are 1, 8 and 1

### Requirement: Per-round seeding
At the start of every round the generator state SHALL be set to that round's seed. Unless seeds are supplied from outside, the seed of round r SHALL be the low 32 bits of the SplitMix64 output for base seed b and index r: z = b + (r+1) × 0x9E3779B97F4A7C15; z = (z xor (z >> 30)) × 0xBF58476D1CE4E5B9; z = (z xor (z >> 27)) × 0x94D049BB133111EB; z = z xor (z >> 31), all in 64-bit wrap-around arithmetic. The base seed is chosen once per game by whoever starts it. (research 01 §7.2)

#### Scenario: Seeds from base 0
- **WHEN** the base seed is 0
- **THEN** rounds 0, 1 and 2 are seeded with 0x7B1DCDAF, 0xA1B965F4 and 0x8009454F

#### Scenario: Seeds from base 1
- **WHEN** the base seed is 1
- **THEN** round 0 is seeded with 0x89025CC1

### Requirement: Externally supplied seeds and samples
The game SHALL allow an outside party to observe each round's seed and each movement sample as it is consumed, and to replace them: a supplied seed is used instead of the derived one, and a supplied direction and fire state are used instead of the resolved controls for that movement sample. With the same supplied values the game SHALL behave exactly as the game in which they were observed. (research 01 §7.2, research 05 §1.6, §4.3)

#### Scenario: Re-running from observed values
- **WHEN** a game's round seeds and movement samples are observed and a second game with the same configuration is fed those values instead of live controls
- **THEN** the second game ends with the same scores, level and tick count

### Requirement: Pause
A pause request on a main-phase tick frame SHALL take effect after that tick completes: the pause cue is emitted, the top line is cleared and `PRESS ANY KEY` is shown at (80,0) in text colour 1. While paused, frames are waiting frames and nothing in the simulation changes, including the gauntlet clock. Pause requests on frames that are not main-phase ticks SHALL be ignored. (research 01 §3.3)

#### Scenario: Paused frames do not simulate
- **WHEN** a game is paused for 200 frames and resumed
- **THEN** the tick count, entity positions and random-number state are the same as when the pause began

#### Scenario: Pause requested during the wind-down
- **WHEN** a pause request arrives on a wind-down tick
- **THEN** the game does not pause

### Requirement: Resuming from pause
A pause request while paused SHALL resume play on that frame: the top line is cleared, each digger's score is refreshed with a 0-point scoring call, the lives display is redrawn and the resume cue is emitted. The next frame is the next main-phase tick. (research 01 §3.3)

#### Scenario: Resume
- **WHEN** a paused game receives a pause request
- **THEN** `PRESS ANY KEY` is gone, scores and lives are shown again, and the following frame is a tick frame

### Requirement: Quit
A quit request on any frame SHALL end the game on that frame. The game reports that it finished because it was quit, with the scores, level and tick count reached; no game-over notice is shown. (research 01 §3.4)

#### Scenario: Quit during play
- **WHEN** a quit request arrives at tick count 300
- **THEN** the game finishes with reason "quit" and reports 300 ticks

### Requirement: Music requests
The game SHALL request the main music at every round start, after bonus mode ends and when a digger respawns outside bonus mode; the bonus music as defined in `digger`; and the dirge on a digger's first tombstone frame. It SHALL request music off at the start of the wind-down. A main-music request SHALL be marked "last life" when the current player's first digger has exactly 1 life, so a different tune can be played. (research 04 §3.1, research 01 §3.2)

#### Scenario: Last life
- **WHEN** a round starts with the player holding exactly 1 life
- **THEN** the main-music request is marked "last life"

#### Scenario: Bonus life changes the tune
- **WHEN** a player on their last life earns a bonus life during a round outside bonus mode
- **THEN** a main-music request without the "last life" mark is emitted

### Requirement: Sound commands per frame
Sound cues SHALL be delivered as an ordered list of commands attached to the frame in which the causing event happened, in the order the events occurred within the frame. Sound commands flow one way: nothing about their playback SHALL feed back into the game. (research 04 §1.5, §5)

#### Scenario: Emerald and gold on one tick
- **WHEN** on one tick digger 1 eats an emerald and, later in the tick's order, a bag breaks
- **THEN** that frame's commands contain the emerald click, the emerald note and the break cue in that order

### Requirement: Finished-game summary
When the game finishes, the frame's outcome SHALL state the reason (all players out, time up, or quit) and a summary: each score's visible and total value, the level of the player whose turn it was, and the tick count. Further step requests after the game has finished SHALL change nothing and report the same outcome. (research 01 §1.1, §3.2)

#### Scenario: Stepping a finished game
- **WHEN** a step is requested after the game has reported it finished
- **THEN** the picture, state and summary are unchanged and no sound command is produced
