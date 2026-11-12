# scoring Specification

## Purpose
Scoring turns game events into points, bonus lives and the score shown on the status line. It also owns the "penalty" counter, through which scoring and drawing work slows the monsters down — a coupling that must be exact for a game to be reproducible.

## Requirements

### Requirement: Score event values
The game SHALL award: 25 points for an emerald; 250 more for the eighth emerald of a run; 500 for collecting gold; 250 for a monster killed by a fireball; 250 for a monster killed by a bag; 1000 for the bonus (cherry); and 200 × multiplier for a monster eaten in bonus mode. No other event scores: crushed diggers, monsters removed by a grave, emeralds destroyed by hobbins or bags and gold taken by monsters score nothing. (research 01 §6.1, research 02 §5.9)

#### Scenario: Event table
- **WHEN** a player eats one emerald, collects one gold pile, shoots one monster and takes the bonus, starting from 0
- **THEN** the score is 25, then 525, then 775, then 1775

#### Scenario: Unscored removal
- **WHEN** a hobbin destroys an emerald
- **THEN** no score changes

### Requirement: Who is credited
Points for emeralds, gold, the bonus, eaten monsters and fireball kills SHALL go to the digger that caused them. In a one-digger game a bag kill SHALL be credited to the player whose turn it is. In an alternating two-player game each player has a separate score. Scores SHALL be 0 at the start of every game. (research 01 §6.1, §6.2)

#### Scenario: Fireball credit with two diggers
- **WHEN** digger 2's fireball kills a monster
- **THEN** digger 2's score rises by 250 and digger 1's is unchanged

### Requirement: Bag kills with two diggers
With two simultaneous diggers a monster killed by a bag SHALL be scored as 125 points to each digger. If digger 1 has no lives left, digger 1 receives 0 and digger 2 receives 250 if it has lives, otherwise 0; otherwise, if digger 2 has no lives left, digger 1 receives 250 and digger 2 receives 0. A scoring call SHALL be made for both diggers in every case, including calls of 0 points. (research 01 §6.1)

#### Scenario: Both diggers in the game
- **WHEN** a bag kills a monster while both diggers have lives
- **THEN** each digger's score rises by 125

#### Scenario: Digger 2 out of lives
- **WHEN** a bag kills a monster while digger 2 has 0 lives
- **THEN** digger 1's score rises by 250 and a 0-point scoring call is still made for digger 2

### Requirement: Monster-eating progression
Each digger SHALL have an eat multiplier starting at 1. Each monster it eats scores 200 × multiplier and then doubles the multiplier, giving 200, 400, 800, 1600, 3200, 6400. The multiplier SHALL be reset to 1 at the start of every round, when the digger respawns, and for every digger when bonus mode starts. It is not reset when bonus mode ends. (research 01 §6.1, research 02 §3.8)

#### Scenario: Three monsters in one bonus mode
- **WHEN** a digger eats three monsters during one bonus mode
- **THEN** it scores 200, 400 and 800

#### Scenario: New bonus mode
- **WHEN** a second bonus mode starts later in the same game
- **THEN** the first monster eaten in it scores 200

### Requirement: Score awards are 16-bit signed values
A single award SHALL be reduced to a signed 16-bit value before it is added, and the eat multiplier SHALL itself be a signed 16-bit value doubled with wrap-around. An award of 51200 (multiplier 256) is therefore added as −14336. (research 01 §6.2, §9)

#### Scenario: Ninth consecutive eat
- **WHEN** a digger's multiplier is 256 and it eats a monster
- **THEN** its score changes by −14336

### Requirement: Bonus life thresholds
Each score SHALL have a next-bonus threshold, starting at the bonus step (20000 unless custom level data sets another). After every scoring call, if the score is at least the threshold, a bonus is awarded (subject to the life cap) and the threshold rises by one step. At most one step SHALL be processed per scoring call. (research 01 §6.2)

#### Scenario: First bonus life
- **WHEN** player 1's score goes from 19990 to 20015 with 3 lives
- **THEN** the player has 4 lives and the next threshold is 40000

#### Scenario: One step per call
- **WHEN** a single award takes a score from 19000 past 40000
- **THEN** one bonus is awarded by that call and the second is awarded by the next scoring call, even one of 0 points

### Requirement: Second score is one point late
For the second score (player 2 of an alternating game, or digger 2), the bonus SHALL be awarded only when the score is at least the threshold plus 1. This reproduces a bug of the original game. (research 01 §6.2, §10 item 1)

#### Scenario: Exactly on the threshold
- **WHEN** player 2's score becomes exactly 20000
- **THEN** no bonus life is awarded yet, and it is awarded by the next scoring call that finds the score at 20001 or more

### Requirement: Life cap
A bonus life SHALL be given only if the digger has fewer than 5 lives, unless unlimited lives is configured. The threshold SHALL advance whether or not a life was given. Giving a life emits the extra-life cue, redraws the lives display, and re-requests the main tune if the life changes which tune applies. (research 01 §6.2, research 04 §3.1)

#### Scenario: At the cap
- **WHEN** a player with 5 lives crosses a threshold
- **THEN** the life count stays 5 and the threshold still rises by one step

#### Scenario: Unlimited lives option
- **WHEN** unlimited lives is configured and a player with 5 lives crosses a threshold
- **THEN** the player has 6 lives

### Requirement: Bonus time in gauntlet mode
In gauntlet mode a bonus award SHALL add 15 seconds (17,897,715 clock units, with 32-bit wrap-around) to the gauntlet clock instead of giving a life, under the same fewer-than-5-lives condition. (research 01 §6.2, §7.4)

#### Scenario: Crossing 20000 in gauntlet
- **WHEN** a gauntlet player's score crosses 20000
- **THEN** the clock value rises by 17,897,715 and the life count is unchanged

### Requirement: Rollover at one million
When a score exceeds 999,999 after an award, the whole score SHALL be moved to a carried total and the visible score SHALL become 0. The bonus threshold is not reset, so no further bonus lives follow. The total score of a player SHALL be carried total plus visible score. (research 01 §6.2, §10 item 3)

#### Scenario: Passing one million
- **WHEN** a score of 999,900 receives 250 points
- **THEN** the visible score is 0, the total score is 1,000,150 and subsequent awards count up from 0 on screen

### Requirement: Score display
Every scoring call SHALL redraw that score on the status line in text colour 1 as six digit positions of 12 pixels each: the five rightmost always drawn with leading zeros, the leftmost only when non-zero. The first score is drawn from x = 0. The second score, shown in alternating two-player and two-digger games, is drawn from x = 236 while below 100,000 and from x = 248 otherwise. (research 01 §6.2, research 03 §8.5)

#### Scenario: Small score
- **WHEN** player 1's score is 25
- **THEN** the digits `00025` occupy x = 12..71 and nothing is drawn at x = 0..11

#### Scenario: Six digits
- **WHEN** player 1's score is 123,450
- **THEN** the digits `123450` occupy x = 0..71

### Requirement: Zero-point scoring calls
A scoring call of 0 points for each digger in play SHALL be made at the start of every round, after a pause ends and when a player's game ends. Such a call SHALL behave like any other: it redraws the score, performs the bonus threshold check and adds to the penalty counter. (research 01 §3.2, §3.3, §6.2, §6.3)

#### Scenario: Round start refresh
- **WHEN** a round starts
- **THEN** each digger's score is redrawn and its bonus threshold is checked once

### Requirement: Penalty counter
A penalty counter SHALL be reset to 0 at the start of every tick and raised by 1 for each of these during the tick: each earth dig step (by a digger, by a hobbin on every move even without a direction, by a bag fall step); each sprite draw listed in the penalty table of the scenarios below; and 3 for every scoring call, including calls of 0 points. (research 01 §4, research 02 §6)

#### Scenario: Penalty table for diggers and fireballs
- **WHEN** a tick is processed
- **THEN** the counter rises by 1 for each: digger move draw (also when standing still); redraw during a push pause; redraw after a blocked or failed push; fireball flight draw; each explosion frame; each tick a dead digger is carried by a bag; each death-arc tick; each tombstone frame; emerald redraw; emerald eaten (by a digger or a hobbin)

#### Scenario: Penalty table for monsters and bags
- **WHEN** a tick is processed
- **THEN** the counter rises by 1 for each: monster move draw; every monster overlapped by a moving monster; monster redraw after a blocked move; each tick a dead monster is carried; each gold opening frame; each wobble redraw; bag landing; redraw of a falling bag pushed sideways; fall-step draw; slide-step draw; each restore of a bag after a failed push (up to two per push); gold collection

#### Scenario: Digger eats an emerald
- **WHEN** on one tick a moving digger eats an emerald that is not the eighth of a run, and nothing else happens
- **THEN** the counter is 6: one dig step, one draw, one for the emerald and three for the scoring call

### Requirement: Penalty overflow delays monsters
In the main phase, after diggers, monsters and bags have been updated, if the counter exceeds 8 then with n = min(counter − 8, 6) the delay of every monster slot numbered 1 to n − 1 SHALL be raised by 1, whether or not the slot is occupied. Slot 0 is never delayed this way and an excess of exactly 1 delays nothing. No such delay is applied during the wind-down. (research 01 §4, research 02 §6, §9 item 1)

#### Scenario: Overflow table
- **WHEN** the counter at the end of a tick is 8, 9, 10, 12 or 20
- **THEN** the slots delayed are none, none, slot 1, slots 1 to 3, and slots 1 to 5 respectively

#### Scenario: Slot 0 exempt
- **WHEN** the counter is 30 and monsters occupy slots 0 and 1
- **THEN** only the monster in slot 1 gains a tick of delay

### Requirement: Reported results
At any time, and in the summary when a game finishes, the game SHALL report for each score its visible score and its total score (carried plus visible), together with the level of the player whose turn it is and the tick count. (research 01 §1.1, §6.2)

#### Scenario: Summary of a finished one-player game
- **WHEN** a one-player game ends on level 4 with 12,375 points after 5,210 ticks
- **THEN** the summary reports score 12,375, level 4 and 5,210 ticks
