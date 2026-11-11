# Spec Delta

## Purpose

How the player's keys, gamepads and typed letters become game controls: the actions that exist, their default bindings, how held keys resolve to one direction, and how bindings are changed.

## ADDED Requirements

### Requirement: Action table
The program SHALL know exactly these 20 bindable actions: `p1_right`, `p1_up`, `p1_left`, `p1_down`, `p1_fire`, `p2_right`, `p2_up`, `p2_left`, `p2_down`, `p2_fire`, `speed_up`, `speed_down`, `toggle_music`, `toggle_sound`, `quit`, `pause`, `change_mode`, `save_recording`, `redefine_keys` and `fullscreen`. An action can have several keys; a key belongs to at most one action. (research 05 §1.1; adding `fullscreen` and dropping the cheat action are port decisions)

#### Scenario: Every action has a default
- **WHEN** the program starts without a configuration file
- **THEN** each of the 20 actions is bound to at least one key and no key is bound to two actions

### Requirement: Key names
Keys SHALL be identified by their physical position on the keyboard, not by the character the current layout prints, and named with the W3C UI Events code names: `ArrowLeft`, `KeyA`, `Digit1`, `F1`, `Space`, `Tab`, `NumpadAdd`, `ControlRight` and so on. These names are used in the configuration file and on the redefinition screen. (port decision)

#### Scenario: French keyboard layout
- **WHEN** a player with an AZERTY layout uses the default player-2 keys
- **THEN** the keys in the positions labelled W, A, S and Z on a US keyboard move the second Digger

### Requirement: Default keys for player 1
By default player 1 SHALL move right, up, left and down with `ArrowRight`, `ArrowUp`, `ArrowLeft` and `ArrowDown`, and fire with `F1`. (research 05 §1.1)

#### Scenario: Moving and firing
- **WHEN** the player holds `ArrowLeft` during a one-player game and then presses `F1`
- **THEN** Digger moves left and fires to the left

### Requirement: Default keys for player 2
By default player 2 SHALL move right with `KeyS`, up with `KeyW`, left with `KeyA` and down with `KeyZ`, and fire with `Tab`. These keys control the second Digger in the two simultaneous modes only. (research 05 §1.1 and §1.6)

#### Scenario: Two diggers on one keyboard
- **WHEN** in a two-player simultaneous game one player holds `ArrowUp` and the other holds `KeyS`
- **THEN** the first Digger moves up and the second moves right on the same ticks

### Requirement: Default function keys
By default the function actions SHALL be bound as follows: `speed_up` to `NumpadAdd` and `Equal`; `speed_down` to `NumpadSubtract` and `Minus`; `toggle_music` to `F7`; `toggle_sound` to `F9`; `quit` to `F10`; `pause` to `Space`; `change_mode` to `KeyN`; `save_recording` to `F8`; `redefine_keys` to `KeyK`; `fullscreen` to `F11`. (research 05 §1.1; the main-row `Equal` and `Minus` keys and `F11` are port decisions)

#### Scenario: Speed keys on a laptop
- **WHEN** the player presses `Equal` (the main-row +/= key) during a game at the default speed
- **THEN** the frame time becomes 70000 µs

### Requirement: Last pressed direction wins
While several direction keys of one player are held, the direction of the key pressed most recently SHALL be the one in effect. When two or more direction keys of one player go down within the same frame, the order of precedence SHALL be right, then left, then down, then up. (research 05 §1.6)

#### Scenario: Second key overrides the first
- **WHEN** the player holds `ArrowRight`, and three frames later also presses and holds `ArrowUp`
- **THEN** the direction is right for the first three ticks and up from the tick after `ArrowUp` went down

#### Scenario: Two keys in the same frame
- **WHEN** `ArrowUp` and `ArrowRight` go down between the same two frames
- **THEN** the direction on the next tick is right

### Requirement: Fallback when the active key is released
When the key that set the current direction is released while other direction keys of that player are still held, the direction SHALL fall back to a held key in the precedence right, left, down, up. Releasing a held key that is not the current direction SHALL change nothing. With no direction key held there is no direction. (research 05 §1.6)

#### Scenario: Release with three keys held
- **WHEN** the player holds `ArrowDown`, `ArrowLeft` and `ArrowUp` (pressed in that order) and releases `ArrowUp`
- **THEN** the direction becomes left

#### Scenario: Releasing a key that is not active
- **WHEN** the player holds `ArrowLeft` then `ArrowUp`, and releases `ArrowLeft`
- **THEN** the direction stays up

### Requirement: Short taps are not lost
A direction key that is pressed and released between two frames SHALL give its direction for exactly one tick. This holds even while another direction key stays held; after that tick the held key's direction is in effect again. (research 05 §1.6)

#### Scenario: Tap shorter than a frame
- **WHEN** no key is held and the player taps `ArrowDown` for 20 ms at the default 80 ms frame time
- **THEN** the direction is down for exactly one tick and none on the next

#### Scenario: Tap while another key is held
- **WHEN** the player holds `ArrowRight` and taps `ArrowUp` for 20 ms
- **THEN** the direction is up for one tick and right again on the following tick

### Requirement: Fire latching
Fire SHALL be read once per frame for each player as "held now, or pressed since the previous frame", so a tap shorter than a frame fires once. A fire state sampled on one tick takes effect on the following tick. Holding the fire key SHALL NOT be affected by keyboard auto-repeat. (research 05 §1.7)

#### Scenario: Short tap on fire
- **WHEN** the player taps `F1` for 10 ms between two frames while Digger can fire
- **THEN** exactly one fireball is launched, on the tick after the one that sampled the tap

### Requirement: Controls per game mode
In one-player, gauntlet and alternating two-player games the player-1 controls SHALL drive the Digger in play, for both alternating players in turn. In the two simultaneous modes the player-1 controls drive the first Digger and the player-2 controls the second. All held-key and direction memory SHALL be cleared at the start of every round. (research 05 §1.5 and §1.6)

#### Scenario: Second player's turn in an alternating game
- **WHEN** it is player 2's turn in a two-player alternating game
- **THEN** the arrow keys and `F1` control Digger and the player-2 keys do nothing

#### Scenario: Key pressed during the death sequence
- **WHEN** the player taps `ArrowUp` while Digger's death sequence plays and holds nothing when the next round starts
- **THEN** Digger stands still at the start of the next round

### Requirement: Function actions fire once per press
Each press of a key bound to a function action SHALL trigger that action exactly once, however long the key is held; keyboard auto-repeat SHALL be ignored. Several different function keys pressed within one frame SHALL each trigger their own action once. (research 05 §1.4 and §1.5 describe upstream repeating actions; once per press is a port decision)

#### Scenario: Holding the speed-up key
- **WHEN** the player holds `NumpadAdd` for two seconds at the default speed
- **THEN** the frame time is 70000 µs, not lower

#### Scenario: Two function keys in one frame
- **WHEN** `F7` and `F9` go down within the same frame
- **THEN** music is toggled once and sound is toggled once

### Requirement: Gamepad movement
A gamepad SHALL move its Digger with the directional pad and with the left stick. A stick direction counts as held while the stick is deflected beyond the dead zone in that direction; the default dead zone is 0.5 of full deflection. Pad and stick directions SHALL be combined with that player's keys under the same last-pressed, fallback and tap rules. (port decision; upstream's joystick code is dead, research 05 §1.8)

#### Scenario: Stick at rest with slight drift
- **WHEN** the left stick rests at 0.12 of full deflection to the right
- **THEN** no direction is produced

#### Scenario: Stick pushed up, then also right
- **WHEN** the stick is pushed up beyond the dead zone and is then rolled so that it is beyond the dead zone both up and right
- **THEN** the direction is up at first and right from the tick after the right deflection crossed the dead zone

### Requirement: Gamepad buttons
By default the bottom and left face buttons (`South`, `West`) SHALL fire, `Start` SHALL pause, and `Select` SHALL trigger mode change. On the title screen `South`, `West` and `Start` start a game; any button ends a pause. The buttons bound to fire, pause, mode change and quit can be changed in the configuration file; quit has no default button. (port decision)

#### Scenario: Playing with a pad only
- **WHEN** the player presses `Start` on the title screen, plays with the d-pad and `South`, and presses `Start` during the game
- **THEN** a game starts, Digger moves and fires, and the game pauses on the second `Start`

### Requirement: Gamepad assignment and hot-plug
The first connected gamepad SHALL act as player 1's controls and the second as player 2's; further pads are ignored. Pads can be connected and removed at any time. A newly connected pad takes the lowest free player number. When a pad is removed, its controls SHALL count as released, and a game played by a person SHALL pause. Keyboard controls keep working alongside pads. (port decision)

#### Scenario: Two pads, two diggers
- **WHEN** two pads are connected and a two-player simultaneous game is started
- **THEN** the first pad moves and fires the first Digger and the second pad the second

#### Scenario: Pad unplugged mid-game
- **WHEN** player 1's pad is unplugged while its d-pad is held right
- **THEN** the game pauses with `PRESS ANY KEY`, and after resuming Digger does not move right by itself

#### Scenario: Pad connected while running
- **WHEN** a pad is connected while the title screen is shown
- **THEN** its `Start` button starts a game without restarting the program

### Requirement: Key redefinition flow
The redefine action on the title screen SHALL open a screen headed `PRESS NEW KEY FOR` that asks for player 1's right, up, left, down and fire keys in that order, then (when the selected mode has two Diggers, under `PLAYER 2:`) the same five for player 2. The action being asked for is shown in colour 2 and turns to colour 1 once a key is accepted. The pressed key becomes the action's only binding. (research 05 §1.3)

#### Scenario: Redefining player 1's keys
- **WHEN** the player presses K on the title screen in one-player mode and then presses `KeyD`, `KeyE`, `KeyA`, `KeyX`, `ControlLeft`
- **THEN** player 1 moves right, up, left, down with those four letter keys and fires with the left Control key, and the title screen returns

### Requirement: Redefinition conflicts
A key already given to an earlier action in the same redefinition SHALL be refused and the same action asked again. After the player keys, every function action whose keys clash with a newly chosen key SHALL be asked for; with the "all" variant (`--redefine-keys=all`) every function action is asked for. Alt keys cannot be bound. (research 05 §1.3; refusing Alt is a port decision)

#### Scenario: Same key twice
- **WHEN** the player chooses `KeyA` for right and presses `KeyA` again for up
- **THEN** the second press is refused and the screen keeps asking for up

#### Scenario: Taking a function key
- **WHEN** the player chooses `Space` as player 1's fire key
- **THEN** after the player keys the screen asks for a new pause key

### Requirement: Aborting redefinition
Pressing the key currently bound to quit during redefinition, when quit is not the action being asked for, SHALL abandon the redefinition: all bindings return to what they were before it began and nothing is written. (research 05 §1.3 documents upstream keeping half-changed keys in memory; restoring them is a port decision)

#### Scenario: Abort after two keys
- **WHEN** the player redefines right and up and then presses `F10`
- **THEN** the title screen returns and the arrow keys still move Digger

### Requirement: Redefinition is saved
A completed redefinition SHALL be written to the configuration file at once, as key names under the action names, and SHALL be in effect in the next run of the program. (research 05 §1.2 and §1.3)

#### Scenario: Bindings survive a restart
- **WHEN** the player redefines player 1's fire key to `ControlLeft`, quits and starts the program again
- **THEN** the left Control key fires and `F1` does not

### Requirement: Typing initials
During initials entry only the 26 letters A to Z SHALL be accepted, taken from the character the keyboard layout produces and shown and stored in upper case. Backspace and Delete step back one position. Every other key, including arrow keys, digits and function keys, SHALL be ignored, except that the quit action and the fullscreen chord keep working. (research 05 §1.10; letters only and upper case are port decisions that fix the upstream arrow-key bug)

#### Scenario: Arrow keys type nothing
- **WHEN** the player presses `ArrowRight` and then `m` during initials entry
- **THEN** the first initial is `M` and no `O` appears

#### Scenario: Correcting a letter
- **WHEN** the player types `A`, `B`, presses Backspace and types `C`
- **THEN** the initials so far read `AC`

### Requirement: Choosing initials with a gamepad
During initials entry a gamepad SHALL be able to enter letters: d-pad or stick up and down step the current position through A to Z (starting at A, wrapping at both ends), a fire button accepts the letter, and d-pad left steps back one position. (port decision)

#### Scenario: Entering "CAB" with a pad
- **WHEN** the player presses up twice and fire, then fire, then up once and fire
- **THEN** the initials read `CAB` and entry completes
