# Spec Delta

## Purpose

Defines what the player hears: the sound effects, the level-done jingle and the four tunes of Digger, how they are timed, prioritised and synthesised into samples, how they are switched, paused and scaled, and the one-way command boundary between the simulation and the sound engine.

Terms used below. A **tick** is one sound tick (about 13.74 ms). A **divisor** is a pitch value: the tone frequency is 1193181 divided by it. An effect "sounds divisor D on tick n" when D is the divisor it offers on its n-th tick after starting; on other ticks it offers nothing. Ticks of an effect are counted from 1, the first tick being the one on which its start command takes effect.

## ADDED Requirements

### Requirement: Sound tick derived from the sample rate
All sound timing SHALL be counted in sound ticks. A tick SHALL last N output samples, where N is the sample rate divided by 72.8 and rounded to the nearest integer (606 samples at 44100 Hz, giving 72.77 ticks per second). Ticks SHALL be counted from the sample stream only, never from a wall clock, and the first tick SHALL occur immediately before output sample N-1. (research 04 §1.3)

#### Scenario: Tick length at the default rate
- **WHEN** the sample rate is 44100 Hz
- **THEN** a tick lasts 606 samples and ticks occur before samples 605, 1211, 1817 and so on

#### Scenario: Tick length at 48000 Hz
- **WHEN** the sample rate is 48000 Hz
- **THEN** a tick lasts 659 samples

#### Scenario: Game speed does not change sound tempo
- **WHEN** the game's frame time is changed while a tune plays
- **THEN** the number of samples per tick and the tune's tempo are unchanged

### Requirement: Commands take effect on tick boundaries
Commands received by the sound engine SHALL take effect at the next tick, in the order they were sent, before any effect or tune advances on that tick. A command SHALL therefore be audible no later than one tick (606 samples at 44100 Hz) after the engine receives it. (research 04 §1.3, §5)

#### Scenario: Command applied at the next tick
- **WHEN** an emerald-click command is received 100 samples after a tick
- **THEN** the click starts sounding at the following tick, 506 samples later at 44100 Hz

#### Scenario: Order within one tick
- **WHEN** a fall-start command and then a fall-stop command are received between two ticks
- **THEN** no fall effect is active after the next tick

### Requirement: Tone frequency from a divisor
A tone's frequency in Hz SHALL be 1193181 divided by its divisor using integer division, so every frequency is a whole number of Hz. A divisor SHALL produce a tone only when it is greater than 40 and less than 16384 (0x4000); any other divisor SHALL produce silence. (research 04 §1.2, §2)

#### Scenario: Truncating division
- **WHEN** a tone with divisor 2280 sounds
- **THEN** its frequency is 523 Hz (1193181 / 2280 = 523.3, truncated)

#### Scenario: Divisor at the upper gate
- **WHEN** a tone with divisor 16384 is requested
- **THEN** the output is silent, while divisor 16383 gives a 72 Hz tone

### Requirement: Output format and level
The sound engine SHALL produce a single channel of signed 16-bit samples at a configurable sample rate from 8000 Hz to 192000 Hz, 44100 Hz by default; a rate outside that range SHALL be rejected when the engine is created. At full master volume with filtering off, a full-amplitude tone SHALL swing between +16383 and -16383 and silence SHALL be the value 0. (research 04 §4.1, §4.2, §4.3)

#### Scenario: Peak level of an effect
- **WHEN** the bag-break effect sounds with master volume 100 % and filtering off
- **THEN** every output sample during the tone is +16383 or -16383

#### Scenario: Silence
- **WHEN** nothing is sounding
- **THEN** every output sample is 0

#### Scenario: Unsupported sample rate
- **WHEN** the engine is created with a sample rate of 4000 Hz
- **THEN** creation fails with an error naming the supported range

### Requirement: Square-wave tone shape
Every tone SHALL be a square wave with 50 % duty cycle: for a tone of F Hz the output SHALL have exactly 2 x F level transitions in any whole second of continuous sounding, and each half period SHALL differ from sample rate / (2 x F) by less than one sample. A change of frequency between two consecutive sounding ticks SHALL keep the wave's phase, so it adds no extra transition. (research 04 §1.2, §4.1)

#### Scenario: Transition count
- **WHEN** a 523 Hz tone sounds for 44100 consecutive samples at 44100 Hz
- **THEN** the output changes sign exactly 1046 times

#### Scenario: Phase kept across a pitch change
- **WHEN** the explosion effect moves from divisor 1313 to 1149 at a tick
- **THEN** the half period in progress at that tick is completed at the new frequency and no additional transition appears at the tick boundary

### Requirement: One voice with effects over music
At most one tone SHALL sound at a time. On each tick, if any active effect offers a divisor, the effect tone SHALL sound and the music SHALL be silent for that tick, even when the offered divisor is itself silent (at or above 16384). Music SHALL sound only on ticks where no effect offers a divisor. Music SHALL keep advancing while it is displaced. (research 04 §1.3)

#### Scenario: Effect interrupts the tune
- **WHEN** the normal tune is playing and an emerald-click command arrives
- **THEN** for one tick the output is the 1193 Hz click and not the tune, and the tune continues on the next tick one tick further on

#### Scenario: Gaps in an effect let music through
- **WHEN** the wobble effect is active while the normal tune plays
- **THEN** the tune is heard on every tick except the wobble's blip ticks

### Requirement: Priority among effects
When several effects offer a divisor on the same tick, the one highest in this order SHALL sound: bonus siren, extra life, bag fall, eat monster, fireball flight, explosion, emerald click, gold, bag break, death, wobble, emerald scale. An effect that offers nothing on a tick SHALL NOT block a lower one. All active effects SHALL advance every tick whether or not they are heard. (research 04 §1.3)

#### Scenario: Gold over break
- **WHEN** gold and bag-break effects are both active on a tick
- **THEN** the gold divisor sounds

#### Scenario: Silent tick of a higher effect
- **WHEN** the bag-fall effect is active but offers nothing on a tick on which the gold effect offers 531
- **THEN** divisor 531 sounds

### Requirement: Bag fall effect
From a fall-start command until a fall-stop command, the engine SHALL repeat a 4-tick cycle in which a tone sounds on the third tick only. The divisor SHALL be 1000 in the first cycle and rise by 50 each cycle (falling pitch from 1193 Hz). A fall-start SHALL restart the effect from divisor 1000 and the first tick of a cycle. (research 04 §2)

#### Scenario: First three tones
- **WHEN** a fall effect runs for 12 ticks
- **THEN** it sounds divisor 1000 on tick 3, 1050 on tick 7 and 1100 on tick 11, and offers nothing on the other ticks

#### Scenario: Stop
- **WHEN** a fall-stop command takes effect
- **THEN** the fall effect offers nothing from that tick on

### Requirement: Bag wobble effect
From a wobble-start command until a wobble-stop command, the engine SHALL sound a one-tick blip every 16 ticks, in a 64-tick cycle: divisor 2500 (477 Hz) on tick 16, 3000 (397 Hz) on tick 32, 2500 on tick 48 and 2000 (596 Hz) on tick 64. A wobble-stop SHALL reset the cycle. (research 04 §2)

#### Scenario: One full cycle
- **WHEN** a wobble effect runs for 64 ticks
- **THEN** it sounds 2500, 3000, 2500 and 2000 on ticks 16, 32, 48 and 64 and offers nothing on the other 60 ticks

#### Scenario: Restart after stop
- **WHEN** a wobble effect is stopped on its 20th tick and started again
- **THEN** its next blip is divisor 2500, 16 ticks after the restart

### Requirement: Bag break effect
A break command SHALL sound divisor 15000 (79 Hz) for exactly 3 ticks and then end. (research 04 §2)

#### Scenario: Three low ticks
- **WHEN** a break command takes effect
- **THEN** divisor 15000 sounds on ticks 1, 2 and 3 and the effect is over on tick 4

### Requirement: Fireball flight effect
Each of the two fireballs SHALL have its own flight effect. A fire-start SHALL set its base divisor to 500; a fire-stop SHALL end it. On every second tick the base SHALL grow by one 55th of itself (integer division) and the effect SHALL sound the new base plus a noise value from 0 to base/8 - 1. It SHALL end by itself once the base exceeds 30000. A fire-start for an already flying fireball SHALL reset only the base, not the two-tick rhythm. (research 04 §2)

#### Scenario: First tones of the first fireball
- **WHEN** fireball 0 is the first fireball since the engine was created and its effect runs 12 ticks without restart
- **THEN** it sounds on ticks 2, 4, 6, 8, 10 and 12 with bases 509, 518, 527, 536, 545, 554 and sounding divisors 510, 572, 564, 554, 570, 571

#### Scenario: Restart every frame
- **WHEN** fire-start for the same fireball is received again 6 ticks after the first
- **THEN** the base returns to 500 and the next sounding base is 509, on the next even tick of the original rhythm

#### Scenario: Two fireballs
- **WHEN** both fireballs sound on the same ticks
- **THEN** the tone heard alternates between the two fireballs from one such tick to the next

### Requirement: Sound-only noise generator
The noise used by the fireball flight effect SHALL come from a generator that belongs to the sound engine alone and never reads or advances the game's random numbers. Its 32-bit state SHALL start at 0 when the engine is created; each draw SHALL replace the state by state x 0x15A4E35 + 1 (modulo 2^32) and return the low 31 bits of the state modulo the requested range. (research 04 §2, §5)

#### Scenario: First two draws
- **WHEN** the first two noise values are drawn with ranges 63 and 64
- **THEN** the state becomes 1 and then 22695478, and the values are 1 and 54

#### Scenario: Game randomness untouched
- **WHEN** a game is run twice with the same inputs, once with fireballs sounding and once with the sound engine absent
- **THEN** the game's random number sequence is the same in both runs

### Requirement: Fireball explosion effect
An explode command for a fireball SHALL end that fireball's flight effect and sound a 10-tick rising chirp: starting from 1500, on each tick the divisor SHALL lose one eighth of itself (integer shift) and then sound. When both fireballs explode together the tone heard SHALL alternate between them tick by tick. (research 04 §2)

#### Scenario: Chirp sequence
- **WHEN** an explode command takes effect
- **THEN** divisors 1313, 1149, 1006, 881, 771, 675, 591, 518, 454 and 398 sound on ticks 1 to 10 and the effect is over on tick 11

#### Scenario: Flight effect ends
- **WHEN** fireball 1 is flying and an explode command for fireball 1 takes effect
- **THEN** fireball 1's flight effect offers nothing from that tick on

### Requirement: Bonus siren effect
From a bonus-start command until a bonus-stop command, the engine SHALL sound a two-tone siren in a 16-tick cycle: divisor 1230 (970 Hz) on positions 0 to 5, nothing on 6 and 7, divisor 1513 (788 Hz) on positions 8 to 13, nothing on 14 and 15. The first tick after a start SHALL be position 1. A bonus-stop SHALL reset the position. (research 04 §2)

#### Scenario: First cycle
- **WHEN** a bonus siren runs for 21 ticks
- **THEN** it sounds 1230 on ticks 1 to 5, nothing on 6 and 7, 1513 on ticks 8 to 13, nothing on 14 and 15, and 1230 on ticks 16 to 21

### Requirement: Emerald click effect
An emerald-click command SHALL sound divisor 1000 (1193 Hz) for exactly one tick. (research 04 §2)

#### Scenario: One tick
- **WHEN** an emerald-click command takes effect
- **THEN** divisor 1000 sounds on tick 1 and the effect is over on tick 2

### Requirement: Emerald scale effect
An emerald command carrying a step from 0 to 7 SHALL sound one note of a rising C major scale as seven bursts, each 2 ticks of tone followed by 6 ticks of nothing (56 ticks in all). The divisors for steps 0 to 7 SHALL be 2280, 2032, 1810, 1708, 1522, 1356, 1208 and 1140 (523, 587, 659, 698, 783, 879, 987 and 1046 Hz). A new emerald command SHALL replace a running one. (research 04 §2)

#### Scenario: Burst pattern
- **WHEN** an emerald command with step 0 takes effect
- **THEN** divisor 2280 sounds on ticks 1, 2, 9, 10, 17, 18, 25, 26, 33, 34, 41, 42, 49 and 50, and the effect is over on tick 57

#### Scenario: Eighth consecutive emerald
- **WHEN** an emerald command with step 7 takes effect
- **THEN** the bursts are at 1046 Hz

### Requirement: Gold effect
A gold command SHALL sound two interleaved sweeps for 31 ticks. One value starts at 4000 and loses one sixteenth of itself each tick; the other starts at 500 and gains one sixteenth of itself each tick (integer shifts). Odd ticks SHALL sound the first value and even ticks the second, each read before that tick's change. (research 04 §2)

#### Scenario: Sweep sequence
- **WHEN** a gold command takes effect
- **THEN** the divisors on ticks 1 to 8 are 4000, 531, 3516, 599, 3091, 675, 2717 and 761, tick 31 sounds 582, and the effect is over on tick 32

### Requirement: Eat-monster effect
An eat-monster command SHALL sound three identical rising chirps, each 20 ticks followed by one tick of nothing (63 ticks in all). In a chirp a value starts at 2000 and loses one sixteenth of itself on every tick; a tone sounds on even ticks only. (research 04 §2)

#### Scenario: One chirp
- **WHEN** an eat-monster command takes effect
- **THEN** divisors 1758, 1649, 1360, 1275, 1052, 987, 815, 765, 632 and 593 sound on ticks 2, 4, 6, 8, 10, 12, 14, 16, 18 and 20, and ticks 1, 3, ... 19 and 21 offer nothing

#### Scenario: Three chirps
- **WHEN** the effect runs to its end
- **THEN** the same ten divisors sound again from tick 23 and from tick 44, and the effect is over on tick 64

### Requirement: Death effect
A death command SHALL run a 51-tick effect and stop any tune on its first tick. On ticks 1 to 10 the divisor SHALL be 20000 - 1000 x tick; from tick 11 it SHALL rise by 500 per tick, ending after the tick on which it exceeds 30000. The effect SHALL offer its divisor on all 51 ticks, so it displaces music throughout, but by the divisor gate it is audible only on ticks 4 to 22. (research 04 §2)

#### Scenario: Audible shape
- **WHEN** a death command takes effect
- **THEN** ticks 1 to 3 are silent, ticks 4 to 10 sound 16000 down to 10000 in steps of 1000 (74 Hz rising to 119 Hz), ticks 11 to 22 sound 10500 up to 16000 in steps of 500, and ticks 23 to 51 are silent

#### Scenario: End of the effect
- **WHEN** the effect reaches tick 51 with divisor 30500
- **THEN** it is over on tick 52

#### Scenario: Music stops
- **WHEN** a death command takes effect while the normal tune plays
- **THEN** the tune is stopped and does not resume when the death effect ends

### Requirement: Extra-life effect
An extra-life command SHALL run a 96-tick effect with a counter going from 96 down to 1, one step per tick. On ticks where the counter divided by 3 (integer division) is odd, it SHALL sound divisor counter x 4 + 600; otherwise nothing. This gives 3 ticks of tone and 3 of nothing with rising pitch. (research 04 §2)

#### Scenario: Start of the effect
- **WHEN** an extra-life command takes effect
- **THEN** tick 1 offers nothing, ticks 2 to 4 sound 980, 976 and 972, ticks 5 to 7 offer nothing and tick 8 sounds 956

#### Scenario: End of the effect
- **WHEN** the effect reaches ticks 92 to 96
- **THEN** ticks 92 to 94 sound 620, 616 and 612, ticks 95 and 96 offer nothing, and the effect is over on tick 97

### Requirement: Level-done jingle
A level-done command SHALL play 11 notes of 21 ticks each (231 ticks, 3.17 s). While it plays, no effect or tune SHALL sound or advance. Note divisors D SHALL be 2280, 1810, 1522, 2032, 1708, 1356, 1810, 1522, 1208, 1140, 1140. Each note SHALL be a full-amplitude tone at 1193181/(D+35) Hz, switched on for the first half and off for the second half of each cycle of the beat frequency 1193181/D - 1193181/(D+35) Hz. The jingle SHALL play whether or not any tune has played before. (research 04 §2)

#### Scenario: First note
- **WHEN** a level-done command takes effect
- **THEN** for 21 ticks the output is a 515 Hz tone interrupted 8 times per second

#### Scenario: Note frequencies
- **WHEN** the jingle plays through
- **THEN** the tone frequencies are 515, 646, 766, 577, 684, 857, 646, 766, 959, 1015 and 1015 Hz with beat frequencies 8, 13, 17, 10, 14, 22, 13, 17, 28, 31 and 31 Hz, and the jingle is over on tick 232

#### Scenario: Jingle before any music
- **WHEN** a level-done command is the first command after the engine is created with music switched off
- **THEN** the jingle is audible

### Requirement: Stop-all command
A stop-all command SHALL end every effect, the level-done jingle and any tune on the tick it takes effect, resetting each to its initial state, so that the output is silent until a new command arrives. It SHALL NOT change the sound and music switches, the pause state, the master volume or the noise generator's state. (research 04 §2)

#### Scenario: Everything ends
- **WHEN** a stop-all command takes effect while the bonus tune, a fall effect and a gold effect are active
- **THEN** the output is silent from that tick on

#### Scenario: Jingle cut short
- **WHEN** a stop-all command takes effect on tick 50 of the level-done jingle
- **THEN** the jingle ends and a later music command plays normally

### Requirement: Four tunes
The engine SHALL provide four looping tunes: normal (Popcorn, 145 notes), bonus (William Tell Overture, 160 notes), dirge (Chopin's Funeral March, 24 notes of which the last 12 are rests) and battle (played instead of the normal tune when the player has exactly one life left). A play-music command SHALL start the named tune from its first note on the next tick; a stop-music command SHALL end it and reset its position. (research 04 §3.1)

#### Scenario: Tune starts from the top
- **WHEN** a play-music command for the bonus tune takes effect while the normal tune is at its 40th note
- **THEN** the bonus tune's first note is loaded on the next tick

#### Scenario: Looping
- **WHEN** the normal tune finishes its 145th note
- **THEN** it continues with its first note

### Requirement: Tune note encoding
A tune SHALL be a list of notes, each a pitch divisor and a duration in units. Divisor 32000 (0x7D00) SHALL be a rest. The note divisors and durations SHALL be those of the reference game's tables. Battle-tune pitches other than rests SHALL be transposed up a major second: divisor x 891 + 500, divided by 1000 with integer division. (research 04 §3.1)

#### Scenario: Battle transposition
- **WHEN** a battle-tune note is stored with divisor 2280
- **THEN** it plays with divisor 2031 (587 Hz)

#### Scenario: Rest
- **WHEN** a note with divisor 32000 plays
- **THEN** the tune is silent for the note's whole length and the rest's divisor is not transposed

### Requirement: Tune tempo
Each duration unit SHALL last a whole number of ticks: 6 for normal, 3 for bonus, 10 for dirge, 2 for battle. A play-music command MAY ask for the quick tempo, which multiplies these by 0.7 and truncates (4, 2, 7, 1). A note SHALL occupy its duration times the ticks per unit, plus one tick. (research 04 §3.2, §3.3)

#### Scenario: Normal tune note length
- **WHEN** a note of 2 units plays in the normal tune
- **THEN** it occupies 13 ticks (179 ms)

#### Scenario: Bonus tune note length
- **WHEN** a note of 2 units plays in the bonus tune
- **THEN** it occupies 7 ticks (96 ms)

#### Scenario: Quick dirge
- **WHEN** the dirge is played at the quick tempo
- **THEN** each unit lasts 7 ticks

### Requirement: Note envelope
Each note SHALL shape a volume level from 1 to 50, changed once per tick and carried from note to note and tune to tune (8 when the engine is created). Attack: add the attack step each tick until 50 is reached. Decay: subtract the decay step each tick down to the sustain level. Release, from the tick on which the note's tick count reaches its gate length: subtract the release step each tick down to 1. (research 04 §3.3)

#### Scenario: Attack reaches the maximum
- **WHEN** a normal-tune note (attack step 50) starts with the level at 6
- **THEN** the level is 50 on the note's first tick and decay starts on its second tick

#### Scenario: Release starts at the gate
- **WHEN** a normal-tune note with a gate of 12 ticks reaches its 12th tick at level 8
- **THEN** the level is 7 on that tick and 6 on the next

### Requirement: Envelope parameters per tune
The attack step, sustain level, decay step and release step SHALL be 50, 8, 15, 1 for the normal tune; 20, 20, 10, 4 for bonus; 50, 25, 5, 1 for dirge; 28, 26, 8, 4 for battle. The gate length SHALL be 2 duration units for every note of the normal tune (a fixed staccato) and the note's duration less 1 unit for the other tunes. (research 04 §3.2, §3.3)

#### Scenario: Normal tune, 2-unit note
- **WHEN** a 2-unit note of the normal tune starts with the level at 6
- **THEN** the levels on its 13 ticks are 50, 35, 20, 8, 8, 8, 8, 8, 8, 8, 8, 7, 6

#### Scenario: Bonus tune, 4-unit note
- **WHEN** a 4-unit note of the bonus tune starts with the level at 8
- **THEN** the levels on its 13 ticks are 28, 48, 50, 40, 30, 20, 20, 20, 16, 12, 8, 4, 1

#### Scenario: Battle tune, 4-unit note
- **WHEN** a 4-unit note of the battle tune starts with the level at 8
- **THEN** the levels on its 9 ticks are 36, 50, 42, 34, 26, 22, 18, 14, 10

### Requirement: Music amplitude and pitch limits
A music tone's peak amplitude SHALL be (level - 1) / 49 of the full-amplitude peak, so level 50 is full amplitude and level 1 is silent. A music divisor below 1000 SHALL be raised to 1000 (1193 Hz at most). A rest SHALL be silent but its envelope SHALL still run. (research 04 §1.2, §1.3, §3.3)

#### Scenario: Amplitude at the sustain level of the normal tune
- **WHEN** a normal-tune note sounds at level 8 with master volume 100 % and filtering off
- **THEN** the output swings between +2340 and -2340

#### Scenario: Level 1 is silent
- **WHEN** a note's release brings the level to 1
- **THEN** the output is 0 until the next note raises the level

### Requirement: Death dirge
The dirge SHALL play even when music is switched off, without changing the music switch. Starting it SHALL end the death effect. Its audible part is the 12 notes before the first rest (50 units): at the normal tempo the first rest loads on tick 513 (7.05 s), at the quick tempo on tick 363. On that tick the engine SHALL report that the dirge has finished. The dirge SHALL then continue silently through its rests until stopped. (research 04 §3.2, §3.3, §5)

#### Scenario: Dirge with music off
- **WHEN** music is switched off and a play-music command for the dirge takes effect
- **THEN** the dirge is audible, and after a stop-music command a normal-tune request stays silent because music is still off

#### Scenario: Finish report
- **WHEN** the dirge has been playing at the normal tempo for 513 ticks
- **THEN** a dirge-finished event is reported once and the output is silent from that tick

#### Scenario: Death effect ends
- **WHEN** the dirge starts on tick 12 of a death effect
- **THEN** the death effect offers nothing from that tick on and the dirge is heard

### Requirement: Music switch
Music SHALL be switchable on and off at any time, with an initial state chosen when the engine is created (on by default). While music is off, tunes other than the dirge SHALL be silent and SHALL NOT advance; switching music on SHALL resume the tune where it stopped. Play-music and stop-music commands SHALL still be accepted while music is off. Effects and the level-done jingle SHALL be unaffected by the music switch. (research 04 §3.3, §3.4, §6)

#### Scenario: Position frozen
- **WHEN** music is switched off at the 10th note of the normal tune and on again 200 ticks later
- **THEN** the tune resumes within its 10th note

#### Scenario: Effects still heard
- **WHEN** music is off and a gold command takes effect
- **THEN** the gold effect is heard

#### Scenario: Tune requested while music is off
- **WHEN** a play-music command for the bonus tune arrives while music is off and music is then switched on
- **THEN** the bonus tune plays from its first note

### Requirement: Sound switch
Sound SHALL be switchable on and off at any time, with an initial state chosen when the engine is created (on by default). While sound is off the output SHALL be silent, but commands SHALL still be accepted and effects, jingle and tunes SHALL advance exactly as when sound is on. Switching sound on SHALL also switch music on. Sound SHALL be switchable on when the engine was created with sound off. (research 04 §1.3, §3.4, §6)

#### Scenario: Mute and unmute
- **WHEN** sound is switched off during the bonus siren and on again 16 ticks later
- **THEN** the output is silent for those 16 ticks and the siren resumes at the same position in its cycle as if it had never been muted

#### Scenario: Sound on forces music on
- **WHEN** music is off, sound is switched off and then on
- **THEN** music is on

#### Scenario: Started quiet
- **WHEN** the engine is created with sound off and sound is later switched on during a round
- **THEN** the tune requested at the start of the round is heard at the position it has reached

### Requirement: Pause
While sound is paused the output SHALL be silent and every effect, the jingle and every tune SHALL be frozen: none advances. Commands received during a pause SHALL still take effect at the next tick. Resuming SHALL continue each from the exact tick at which it was frozen. Pausing when already paused, or resuming when not paused, SHALL do nothing. (research 04 §2)

#### Scenario: Freeze and resume
- **WHEN** sound is paused on tick 10 of a gold effect and resumed 500 ticks later
- **THEN** the output is silent during the pause and the first tick after resuming is tick 11 of the gold effect (divisor 2100)

#### Scenario: Stop while paused
- **WHEN** a stop-all command arrives during a pause
- **THEN** nothing sounds when the pause ends

### Requirement: Master volume
The engine SHALL have a master volume from 0 % to 100 %, 100 % by default, changeable at any time. Every output sample SHALL be the unscaled sample multiplied by the volume and divided by 100, truncated toward zero. A value above 100 SHALL be treated as 100. (research 04 §4.3)

#### Scenario: Half volume
- **WHEN** the master volume is 50 % and a full-amplitude effect sounds with filtering off
- **THEN** the output swings between +8191 and -8191

#### Scenario: Zero volume
- **WHEN** the master volume is 0 %
- **THEN** every output sample is 0 while effects and tunes advance normally

### Requirement: Optional output filtering
The engine SHALL offer a filter, on by default and switchable, that passes the signal through a first-order high-pass at 1000 Hz and then a first-order low-pass at 4000 Hz, rounds to the nearest integer and limits the result to the 16-bit range. With w = tan(pi x Fc / Fs) and n = 1/(1+w), each stage SHALL compute y = b0 x + b1 x' - a1 y' (x', y' the previous input and output) with a1 = n(w-1); b0 = n, b1 = -n for the high-pass and b0 = b1 = n w for the low-pass. (research 04 §4.2)

#### Scenario: Filter off
- **WHEN** filtering is off
- **THEN** the output is the unfiltered square wave

#### Scenario: Constant input is removed
- **WHEN** filtering is on and the unfiltered signal holds one value for one second
- **THEN** the output decays to 0

#### Scenario: No overflow
- **WHEN** filtering is on and a full-amplitude 79 Hz tone sounds
- **THEN** every output sample lies between -32768 and +32767

### Requirement: Reproducible output
The engine's output SHALL depend only on its settings and on the commands it receives and the sample positions at which it receives them: the same inputs SHALL give the same tick-by-tick sequence of sounding divisors and music levels on every platform, and with filtering off the same samples. The port SHALL match the reference game at this tick level (same notes, durations, shapes, priorities); it is not required to produce the same samples as the reference build. (research 04 §4.1, §5)

#### Scenario: Same script, same samples
- **WHEN** the same command script is rendered twice with filtering off
- **THEN** the two sample streams are identical

#### Scenario: Tick-level fidelity
- **WHEN** any effect is rendered
- **THEN** its sequence of sounding divisors per tick equals the sequence documented for the reference game

### Requirement: Running without an audio device
The game SHALL run identically when no audio device is available or sound output is disabled: sound commands SHALL be accepted and discarded without error or delay, and no completion event SHALL be required by any other part of the game. If the engine cannot keep up and more than 4096 commands are waiting, further commands SHALL be discarded without blocking the sender. (research 04 §1.4, §5)

#### Scenario: No device
- **WHEN** a game is played with no audio device
- **THEN** the game runs to the same scores and frame counts as with a device, in silence

#### Scenario: Commands pile up
- **WHEN** 5000 commands are sent while no samples are being pulled
- **THEN** sending never blocks, 4096 commands are kept and the rest are discarded and counted

### Requirement: Sound never influences the simulation
No state of the sound engine SHALL be readable by the simulation, and the simulation SHALL never wait for a sound to finish. The pause after a completed level and the pause during the death sequence SHALL be fixed numbers of frames decided by the simulation alone. The same inputs SHALL give the same game whether sound is on, off, paused, muted or absent. (research 04 §5)

#### Scenario: Level-done wait is independent of sound
- **WHEN** a level is completed with sound switched off
- **THEN** the next level starts after the same number of frames as with sound on

#### Scenario: Replay with and without sound
- **WHEN** a recording is replayed once with the sound engine attached and once without
- **THEN** both replays produce the same scores, level, tick count and state checkpoints

### Requirement: Completion events are informational
The engine SHALL report two events to the program's frontend: level-done jingle finished (on tick 232 of the jingle, or when a stop-all cuts it short) and dirge finished. Each SHALL be reported once per occurrence. The events SHALL be available for presentation purposes only and SHALL NOT be an input of the simulation. (research 04 §1.5, §5)

#### Scenario: Jingle finished
- **WHEN** a level-done jingle plays to its end
- **THEN** exactly one jingle-finished event is reported, 231 ticks after the jingle started

#### Scenario: Nobody listens
- **WHEN** the events are never read
- **THEN** the engine's output is unaffected and later events are discarded without error

### Requirement: Sound command boundary
The simulation SHALL emit only these sound commands, in game order within a frame: stop-all; fall-start, fall-stop; wobble-start, wobble-stop; break; fire-start, fire-stop, explode (each naming fireball 0 or 1); bonus-start, bonus-stop; emerald-click; emerald (step 0 to 7); gold; eat-monster; death; extra-life; level-done; play-music (tune normal, bonus, dirge or battle; tempo standard or quick); stop-music. (research 04 §1.1, §2, §3.1)

#### Scenario: Eating an emerald
- **WHEN** a digger eats its third consecutive emerald
- **THEN** the simulation emits emerald-click and then emerald with step 2

#### Scenario: Fireball hits
- **WHEN** fireball 0 hits a monster
- **THEN** the simulation emits explode for fireball 0 and the sound engine needs no separate fire-stop

### Requirement: Frontend sound controls
The sound switch, the music switch, pause and resume, the master volume and the filter switch SHALL be controls sent by the program's frontend. They SHALL NOT be emitted by the simulation, SHALL NOT appear in recordings, and SHALL take effect at the next tick like commands. The frontend SHALL be able to read whether sound and music are currently on. (research 04 §3.4, §6)

#### Scenario: Toggle during a replay
- **WHEN** the music switch is toggled while a recording is replayed
- **THEN** the music stops or resumes and the replay's result is unchanged

#### Scenario: Reading the switches
- **WHEN** music is off and sound is switched off and on again
- **THEN** the frontend reads both sound and music as on within one tick

### Requirement: Tune selection by the simulation
The simulation SHALL name the tune in every play-music command. Where the reference game asks for the normal tune, it SHALL send the battle tune when the player whose turn it is has exactly one life left. When a gained life changes that choice while such a request is current, it SHALL send a new play-music command, which restarts the tune. The dirge SHALL be sent at the standard tempo in one-digger games and at the quick tempo in two-digger games. (research 04 §3.1)

#### Scenario: Last life
- **WHEN** a round starts and the current player has one life left
- **THEN** the play-music command names the battle tune

#### Scenario: Extra life on the last life
- **WHEN** a player with one life left gains a life while the battle tune plays
- **THEN** the simulation emits extra-life and a play-music command for the normal tune

#### Scenario: Two-digger death
- **WHEN** a digger dies in a two-digger game
- **THEN** the play-music command names the dirge at the quick tempo
