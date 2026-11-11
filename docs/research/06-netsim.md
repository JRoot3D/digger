# 06 — NetSim (networked two-player) subsystem: porting reference

Source tree: `https://github.com/sobomax/digger`
(all `file:line` references below are relative to it). Read-only analysis; nothing was built or run.

---

## 1. What NetSim is, from the player's side

**One sentence:** two machines each run the full deterministic Digger simulation in "two diggers on
screen" mode and exchange only *frame number + 1 byte of controls* per frame over UDP, with SIP used
to find each other and set the session up (`digger.txt:476-481`, feature list `digger.txt:80`).

### Enabling it
- Opt-in only, by the `/N` command-line option (`main.c:1076-1086`). Without `/N` the "TWO-PLAYER
  NETSIM" entry is hidden from the title-screen mode cycle (`mode_available()`, `main.c:648-654`;
  mode table `main.c:638-645`). `/N` forces `gauntlet=false, netsim=true, nplayers=1, diggers=2`.
  `/2`, `/G` and the INI "Players" setting all clear `dgstate.netsim` (`main.c:1142-1145, 1172-1178,
  1268-1280`).
- Usage text: `"/N = Enable two-player SIP/RTP NetSim mode (~ preferred, - fallback)"` (`main.c:1127`).
- Two argument forms (`digger.txt:417-426`, parser `netsim_configure()` `netsim.c:2423-2548`):
  1. **Listen / "local registrar" form** `/N:user-peer[@[localip][:port]]` — bind UDP `localip:port`
     (default all addresses, port 5060) and wait for the peer to REGISTER with us. Detected by an
     `@` with a `-` or `~` *before* it and no `:` before it (`netsim.c:2439-2461`). `sip.server_host`
     stays empty, which is what "local registrar mode" means everywhere (`netsim_sip.c:81-86`).
  2. **Client form** `/N:user[:password]@host[:port]-peer` — bind an ephemeral port, REGISTER with
     `host` (which is either the other player running form 1, or a real SIP server/registrar both
     players use), then INVITE `peer` through it. Password is used for SIP digest auth.
  - `~` and `-` both separate the peer name; `~` is searched first (`netsim.c:2462-2464`) so user
    names containing `-` work with `~`. The peer may be omitted entirely (`peer_omitted`,
    `netsim.c:2465`), meaning "anyone" in listen mode.
  - Field limits: user/peer 63 chars, password 31, host 63, port 15 (`netsim_sip.h:17-20`),
    listen host 63 / listen port 7 (`netsim.c:253-254`).
- Example from the tests (`scripts/do-test-netsim.sh:183-187`):
  `alice: /N:alice-bob@:PORT` and `bob: /N:bob@127.0.0.1:PORT-alice`.

### Title screen behaviour
- While the NetSim mode is the selected mode, `sync_netsim_waiter()` → `netsim_sync_enabled(true)`
  is called every title frame (`main.c:436, 448, 727-732`); this lazily starts the network threads
  (`netsim_startup()`, `netsim.c:2557-2649`). Selecting any other mode shuts them down
  (`netsim.c:2651-2666`). A failed startup is retried no sooner than 10 s later
  (`NETSIM_BEGIN_WAIT_RETRY_MS`, `netsim.c:53, 2568, 2642`).
- The second title line ("NETSIM") is coloured by status (`main.c:685-701`): colour 2 = OFF/STARTING,
  3 = RUNNING (socket up, not registered), 1 = REGISTERED (our REGISTER succeeded, or — in listen
  mode — at least one peer has registered with us; `netsim_sip_registration_ready()`
  `netsim_sip.c:1656-1662`).
- The high-score table is replaced by a **FRIENDS** list (`scores.c:301-327`): up to 10 visible rows
  `"%-8.8s %2u"` (name, games played capped at 99), the selected row highlighted. Up/Down move the
  selection (`main.c:457-473`); for that reason Up/Down no longer start the game in NetSim mode
  (`input_set_updown_start(!dgstate.netsim)`, `main.c:441, 494`).
- Starting the game locally calls the *selected friend* (`netsim_friend_selected_name()`,
  `netsim.c:2767`). The top line shows "WAITING FOR PEER" (`main.c:531-533`). If the session cannot
  be established the death jingle plays and the title returns (`main.c:534-546`).
- **An incoming call auto-starts the game** on the callee: there is no accept prompt.
  `netsim_remote_start_requested()` (`netsim.c:3021-3031`) flips `started`/`started_by_remote` in the
  title loop (`main.c:482-485`) and `netsim_start_session(false)` then answers 200 OK.
- In game: each machine uses its *player-1 key bindings* for its own digger whichever simulated
  player it is (`digger.txt:106-107, 479-481`; `input_snapshot_primary_controls()` `input.c:179-197`).
  Caller = simulated player 1 (slot 0), callee = player 2 (slot 1) (`netsim.c:2204`,
  `netsim_sip.c:1171-1172`).
- Pause is shared: either side pausing pauses both; a player with no lives left cannot pause and
  sees "PLAYER n AWAY" when the other pauses (`main.c:807-860`).
- At game over only the local player's score goes for the hall of fame (`scores.c:246-266`).

### Friends list (`netsim_friends.c`, 463 lines)
- Up to 50 entries (`NETSIM_FRIENDS_MAX = NETSIM_SIP_MAX_REGISTERED = 50`, `netsim_friends.c:13`,
  `netsim_sip.h:26`), each `{name, games_played, last_play_ts}`; sorted by `last_play_ts` descending,
  then name bytewise (`netsim_friends.c:124-141`).
- Persisted as text: `$HOME/.digger.frd` on Unix/SDL, `/var/games/digger/digger.frd` on FreeBSD VGL,
  `DIGGER.FRD` elsewhere (`netsim_friends.c:32-53`). Format: first line `DIGGER_FRIENDS 2`, then
  `games\tlast_play_ts\tname\n` per friend (`netsim_friends.c:350-355`); version 1 lines
  `games\tname` are still read (`netsim_friends.c:287-311`).
- Populated from: the `/N` peer name (`netsim_friends_configure`, `:359-376`), any user who
  REGISTERs with us in listen mode (`netsim_friend_registered`, `:378-390`), and any peer a game was
  actually played with — bumped on the first synced frame (`netsim_friend_touch`, `:392-411`, called
  from `netsim.c:2948-2952`). When full, the last (oldest) entry is evicted (`:239-240`).
- Saved at program exit if NetSim was configured or the list is non-empty (`main.c:600-601`).

### Maturity
- Compiled into every native build (Unix and Windows, CMake and GNU make) — there is **no build
  option to leave it out**; only Emscripten gets the stub (`CMakeLists.txt:232-243`,
  `GNUmakefile:182-186`). At run time it is strictly opt-in via `/N`.
- Under very active development: of the 50 commits in this (shallow) checkout, 15 touch `netsim.c`,
  all dated up to 2026-10-14, with subjects such as "give up on the peer after a time without its
  ack, not retries", "SIP ids of its own for each call", "keep what's queued for the next session".
  The protocol payload has a "reserved for future use" subtype (`netsim.c:91`). Treat the wire
  format as **unstable upstream**.
- `Makefile.fbsd:9-11` lists `netsim.c`/`netsim_rx.c` but none of the SIP files — it looks stale and
  probably no longer links. File headers alternate between "Digger Remastered" and "Digger Reloaded".
- It is well tested for what it is: a dedicated CI suite on Linux/macOS/Windows plus ASan/MSan
  (`.github/workflows/*.yml`, `README.md:168-175`), see §6.

---

## 2. Architecture

### 2.1 Model: strict lockstep, no rollback, no input delay
- Every game frame (`syncframe()`, `digger.c:253-283`) the game thread samples local controls, hands
  `(frame, bits)` to the network thread and **blocks until the peer's bits for the same frame number
  arrive** (`netsim_sync_frame()`, `netsim.c:2910-2973`). Both sides then apply both players' bits to
  the same frame. There is no prediction, no rollback, and no configurable input delay; latency
  shows up directly as frame time.
- Frame numbers are a per-game `uint32_t` counter starting at 1 (`resetframe()` sets 0,
  `syncframe()` pre-increments; `digger.c:134, 153-159, 277`). The network layer assumes frames are
  consecutive from 1 (`last_delivered` starts at 0, `netsim.c:1582`).
- *Every* `newframe()` goes through this path, including non-simulation frames: redraw delays
  (`redrawdelay()` `digger.c:299-310`, used by `bags.c:72`, `drawing.c:140`, `digger.c:950`), pause
  frames (`pauseframe()`), level-done jingle frames (`freezeframe()` from `sound.c:330-360`) and the
  end-of-game drain. So the "frame" is a transport tick, not a game tick; `dgstate.ticks` is separate.
- Determinism inputs shared by both peers: the per-round RNG seed is derived from the 64-bit session
  nonce (`seedbase()`/`roundseed()` → `splitmix64(nonce, round)`, `main.c:108-128, 171`), and both
  peers apply identical control bits on identical frames. Nothing else is synchronised.

### 2.2 What is exchanged per frame
One 44-byte UDP datagram per send (see 2.4). Its logical content:
- `frame` (u32), `bits` (u8): `INPUT_CTRL_UP 0x01, DOWN 0x02, RIGHT 0x04, LEFT 0x08, FIRE 0x10`
  (`input.h:26-30`) plus control flags `NETSIM_CTRL_PAUSE 0x20, QUIT 0x40, FREEZE 0x80`
  (`netsim.h:11-13`).
  - FREEZE: "this frame is not a simulation frame for me" (redraw delay, jingle, drain).
  - PAUSE: local pause latch (only honoured if the local player still has lives, `netsim_game.c:81-83`).
  - QUIT: local player quit; both sides set `escape` on the same frame (`netsim_game.c:100-105`).
- `tx_seq` (u32, per-session, starts at 1, +1 per *new* queued frame; retransmits reuse it).
- `unseen_frame`: next frame of the peer's that we have not delivered yet (`last_delivered + 1`),
  sent as a signed 8-bit offset from `frame`.
- Optional **repair**: the previous frame's bits bundled into this packet when the peer says the
  frame it has not seen is exactly our previous one (`thread_fill_send_meta()`, `netsim.c:1854-1876`).
- Timing echo fields for RTT estimation (2.7), session nonce, stream SSRC, player number.

### 2.3 Threads, queues, locking
Three threads when NetSim is on (plus the sound thread, unrelated):
1. **Game/main thread** — public API in `netsim.h`; never touches the socket.
2. **NetSim thread** `netsim_thread()` (`netsim.c:2363-2421`) — owns all protocol state
   (`struct netsim_thread_state`, `netsim.c:1423-1462`) and the SIP stack (microsippy is
   single-threaded and only called from here). Loop: drain SIP events → handle one out-queue item →
   `netsim_sip_run()` → drain SIP events → promote a pending local frame → send/retransmit →
   deliver in-order remote frames → sleep on the out-queue condvar until
   `min(tx.next_tx, SIP refresh/retry time, now+100 ms)` (`NETSIM_IDLE_WAKE_MS`, `netsim.c:46,
   1472-1481, 2407-2408`). SIP transaction timers are *not* part of the wake-up computation; they
   are serviced by the 100 ms idle wake (`netsim_sip.c:1434-1436`).
3. **RX thread** `netsim_rx_thread()` (`netsim_rx.c:9-71`) — blocking `recvfrom` with a 100 ms
   `SO_RCVTIMEO` (`netsim_platform.c:970`), `malloc`s a 4096-byte buffer per datagram, timestamps it
   with the monotonic clock and posts `NETSIM_INT_RX_PACKET` to the out-queue. Fatal socket errors
   become `NETSIM_INT_RX_ERROR`.

Two bounded ring queues of 32 entries (`NETSIM_QUEUE_LEN`, `netsim_rx.h:12`), each a mutex +
condvar, with **blocking put when full** (back-pressure):
- **out-queue** (game → net *and* rx → net): `HELLO 100 (unused), FRAME 101, STOP 103, START 104,
  START_ACK 105, SHUTDOWN 106, INT_RX_PACKET 107, INT_RX_ERROR 108, INT_ABORT 109`
  (`netsim_rx.h:15-25`).
- **in-queue** (net → game): `START 201, FRAME 202, EXIT 203, ERROR 204 (+ why[160]), GAMESTART 205,
  START_ACK 206, FRIEND_REGISTERED 207` (`netsim.c:56-64, 127-137`).
- Session teardown filters the out-queue: everything is dropped except a queued `SHUTDOWN` and
  anything from the *next* session's `START`/`START_ACK` onward (`queue_out_clear_session()`,
  `netsim.c:698-729`) — the subject of a recent bug fix.
- Atomics: `g_title_status`, `g_net_frame` (for log lines), `g_nonce_seq`, `rx.stop_requested`.
- Unsynchronised by design: `g_session` (state machine, local player, nonce) is only touched by the
  game thread; the friends list likewise (the net thread forwards `FRIEND_REGISTERED` through the
  in-queue). Benign race: `netsim_log()` on the net thread reads the game's non-atomic `frame` and
  `dgstate.ticks` (`netsim.c:471-485`).
- `spinlock.c/.h` (BSD-2, Sippy Software; `atomic_flag` busy-wait lock, `spinlock.c:41-104`) is
  **not used by NetSim proper**; its users are `sound.c` and `digger_log.c` (the logger shared by
  all threads). It belongs with the sound/logging port.

### 2.4 Wire format — game ("media") packets
RTP-shaped but not real RTP semantics. Fixed size **44 bytes**; anything else is discarded
(`decode_packet()`, `netsim.c:967-1010`; static asserts `netsim.c:1467-1470`). All multi-byte fields
big-endian.

| Off | Size | Field | Notes |
|---:|---:|---|---|
| 0 | 1 | `vpxcc` | `0x80` (V=2, P=X=CC=0); low 6 bits must be 0 |
| 1 | 1 | `mpt` | payload type 96, marker 0; receiver masks `0x7f` |
| 2 | 2 | RTP seq | low 16 bits of `tx_seq` |
| 4 | 4 | RTP timestamp | **the frame number** |
| 8 | 4 | RTP SSRC | sender's *stream* SSRC for frames (control SSRC only for the unused hello/start types, `netsim.c:1201-1218`) |
| 12 | 1 | `subtype` | must be 0 (`NETSIM_PKT_FRAME`) |
| 13 | 1 | `player` | sender's simulated player, 0 or 1 |
| 14 | 2 | `tx_seq_hi` | high 16 bits of `tx_seq` |
| 16 | 4 | `bits` | bits 0-7 = this frame's bits; bits 8-15 = previous frame's bits if repair flag set |
| 20 | 8 | `nonce` | session nonce (see byte-order caveat below) |
| 28 | 4 | `stream_ssrc` | sender's stream SSRC again (decoded, not otherwise checked) |
| 32 | 2 | `hold_ms` | ms between receiving the peer's last packet and sending this one |
| 34 | 1 | `unseen_off` | int8: `unseen_frame - frame`; decode rejects result ≤ 0 |
| 35 | 1 | `reserved` | bit 0 = `NETSIM_PKT_REPAIR_PREV` |
| 36 | 4 | `echo_peer_ct_ms` | peer's send-clock estimate being echoed (0 = none) |
| 40 | 4 | `echo_local_tor_ms` | our monotonic ms when the peer's last packet arrived (0 = none) |

Struct definitions `netsim.c:83-104`; encode `send_packet()` `netsim.c:928-965`.

**Byte-order caveat (latent upstream bug):** `netsim_htonll()` (`netsim.c:559-567`) byte-swaps each
32-bit half *and* swaps the halves. On little-endian hosts that yields a correct big-endian u64; on
a big-endian host `htonl` is the identity so the halves are swapped on the wire. A Rust port should
emit `nonce.to_be_bytes()`, which matches every little-endian C build (i.e. all that matter).

Receive-side acceptance filter, in order (`thread_process_rx_packet()`, `netsim.c:2019-2118`):
1. If it "looks like SIP" — starts with `SIP/2.0` or three ASCII letters
   (`netsim_sip_packet_looks_like()`, `netsim_sip.c:1644-1654`) — hand to the SIP stack. **SIP and
   media share one UDP socket/port**; a media packet starts with `0x80`, so there is no ambiguity.
2. Must decode; session must be active; `nonce == session_nonce`; RTP SSRC must equal the peer
   stream SSRC learned from SDP (if non-zero); `player == remote_player`.
3. Source address is latched from the first valid packet (`media_source`); later packets from any
   other address are dropped. Note sends still go to the SDP address (`media_target`), not to the
   latched source — there is no symmetric-RTP behaviour.
4. `tx_seq < last_peer_tx_seq` → dropped.

### 2.5 Reliability protocol (the subtle part)
State per session: `tx` (current pending frame), `prev_tx`, one-deep `pending_frame` holding slot,
32-slot remote frame window, 32-slot "times I saw peer frame N" counters, 4-slot peer-seq reorder
window (`netsim.c:28-29, 212-247, 1423-1462`).

- **Sending.** A new local frame becomes `tx` with the next `tx_seq` (`thread_set_tx()`,
  `netsim.c:1731-1748`); the old `tx` is kept as `prev_tx`. If the current `tx` is still unresolved
  the new frame waits in `pending_frame` (a second one would be dropped with a log line —
  `netsim.c:1616-1629`; cannot happen in practice because the game blocks).
- **Implicit acks, no ack packets.** Because both sides advance `tx_seq` by one per frame from 1:
  - peer packet with `tx_seq > tx.tx_seq` ⇒ the peer has moved on, so it has our frame: `tx`
    deactivated (`thread_ack_tx()`, `netsim.c:1712-1729`);
  - peer packet with *the same* `tx_seq` and frame ⇒ "matched": timer-driven retransmission stops
    (`thread_mark_matching_peer_tx()`, `netsim.c:1838-1852`) and the next local frame may go out.
- **Retransmission.** Timer-driven with back-off: first interval = smoothed RTT rounded up to ms,
  min 10 ms (`thread_retry_ms()`, `netsim.c:1540-1560`), doubling up to a cap of sync-timeout/4
  (`pending_retry_ms()`, `netsim.c:1300-1322`). After a match, retransmission becomes
  **peer-driven**: a duplicate from the peer for frame N triggers one resend of our frame N only
  while `send_count < peer_frame_count` ("parity gate", `thread_resend_to_match()`,
  `netsim.c:1878-1901`). A timer resend is also unscheduled once we have sent at least as many
  copies as we have received since queuing (`netsim.c:1413-1418`).
- **Forward repair.** If the peer's `unseen_frame` equals our previous frame, the current packet
  carries the previous frame's bits too (`netsim.c:1868-1875`, `950-953`); the receiver stores both
  and marks the repaired sequence number as seen (`netsim.c:1772-1789, 2109-2117`).
- **Receive ordering.** Sequence numbers are accepted within a window of 4 ahead of the last
  in-order one; beyond that the session **fails** with "peer seq gap"
  (`thread_accept_peer_seq()`, `netsim.c:1756-1813`, `2062-2071`). Frames more than 32 ahead of
  `last_delivered` fail it with "frame window overflow" (`netsim.c:2100-2108`).
- **Delivery.** Remote frame N is pushed to the game only when it is the next in order *and* our
  own frame N has been sent at least once (needed to compute the lead, `netsim.c:2138-2169`).
- **Sync timeout.** A frame not acked within `NETSIM_SYNC_TIMEOUT_MS = 1000` ms of its first send
  attempt fails the session: "retransmit timeout: frame N not acked in M ms (K sends)"
  (`netsim.c:34, 1253-1264, 1402-1408`). Overridable only in instrumented builds via
  `DIGGER_NETSIM_SYNC_TIMEOUT` (`netsim.c:1228-1251`); CI on emulated platforms uses 5000
  (`docker/Dockerfile.build:75`).
- **Slow start (callee only).** Until the caller's first media packet or its SIP ACK arrives, the
  callee resends its first frame every 100 ms and the sync timeout does not apply — SIP's own timers
  decide (`thread_slow_start()`/`thread_end_slow_start()`, `netsim.c:1503-1538`,
  `NETSIM_SLOW_START_RETRY_MS` `:37`). On leaving slow start the timeout clock restarts.
- **Send errors.** Transient errors (`ECONNREFUSED, EHOSTDOWN, EHOSTUNREACH, ENETDOWN, ENETUNREACH,
  ENOBUFS`; `netsim_platform.c:1141-1156`) count as loss and are retried; anything else fails the
  session (`netsim.c:1349-1376`).

### 2.6 Session establishment (SIP/SDP)
The SIP stack is microsippy (§7); `netsim_sip.c` (1784 lines) is the glue.

Modes:
- **Client mode** (server host given): REGISTER to the server on creation, `Expires: 300`, digest
  auth with `qop=auth` if challenged (`netsim_sip.c:1288-1296, 748-768, 876-897`); failed/timed-out
  registration retried every 5 s (`NETSIM_SIP_REGISTER_RETRY_MS`, `:46, 770-775`). Incoming requests
  are accepted **only from the server's address**, else 403 (`netsim_sip.c:1006-1010`). INVITEs go to
  the server (`server_target()`, `:88-98, 804-806`).
- **Local registrar mode** (no server host): we are a minimal registrar. An incoming REGISTER is
  answered 200 with `Contact: <uri>;expires=N` and its Contact host/port stored as a binding, max 50,
  expiring per `expires` (`netsim_sip_registrar.c:106-246`; expiry sweep `netsim_sip.c:1366-1373`).
  **No authentication.** If a peer name was configured, only that user may register (else 403,
  `netsim_sip_registrar.c:140-142`, `netsim_sip.c:986-1004`). A call to a peer with no binding yet is
  deferred until its REGISTER arrives (`netsim_sip.c:1546-1553, 1387-1407`). INVITE goes straight to
  the stored Contact.

Call flow (caller = player 1):
1. Game thread: `netsim_start_session(true)` makes a 64-bit nonce (`make_nonce()`: monotonic ns ^
   pid<<32 ^ counter*golden-ratio ^ an address; `netsim.c:865-877`) and queues `OUT_START`.
2. Net thread: `thread_begin_session()`, computes local stream SSRC, queues the INVITE with an SDP
   offer (`netsim.c:2199-2237`, `start_pending_call()` `netsim_sip.c:777-865`). INVITE carries
   `Expires: 60`; Timer B shortened to 5 s (`NETSIM_SIP_INVITE_TIMER_B_MS`, `:47, 788-790`).
3. Callee: request must target our user name (else 404), UA must be idle (else stateless 486 Busy
   Here), SDP must parse and say `player == 0` (else 488) (`netsim_sip.c:1038-1072, 1146-1199`).
   Emits `REMOTE_START` → in-queue `START` + `GAMESTART` → title loop starts the game →
   `netsim_start_session(false)` queues `OUT_START_ACK` → 200 OK with SDP answer
   (`answer_incoming_invite()`, `netsim_sip.c:670-723`).
4. Caller validates the answer: same session nonce, `player == 1` (`netsim_sip.c:1213-1227`), ACKs.
   Both sides emit `CONNECTED` → in-queue `START_ACK` → `NETSIM_SESSION_STARTED`.
5. Caller side gives up after 5 s without a start-ack (`netsim.c:2808-2813`); callee waits
   indefinitely for the events (`netsim.c:2815`) — SIP errors/timeouts break it out.
6. Teardown: BYE via `netsim_sip_hangup()` (`netsim_sip.c:1601-1642`). Who hangs up is negotiated in
   game logic (§3.3).

Game-thread session state machine: `WAITING, ASSIGNED, REMOTE_START, READY_TO_ACK, START_ACK,
STARTED, ERROR, PEER_EXITED` (`netsim.c:257-266`, transitions `netsim.c:2686-2729`).

SDP body, exactly as built (`netsim_sip_sdp.c:59-80`), CRLF line ends:
```
v=0
o=- <nonce-decimal> 1 IN IP4 <host>
s=digger-netsim
c=IN IP4 <host>
t=0 0
m=application <port> RTP/AVP 96
a=rtpmap:96 digger/11932
a=x-digger-session:<nonce as unsigned 64-bit decimal>
a=ssrc:<stream ssrc decimal> cname:digger-netsim
a=x-digger-player:<0|1>
```
The parser (`netsim_sip_sdp.c:82-146`) requires all of: `c=IN IP4 `, `m=application <port> RTP/AVP…`,
`a=x-digger-session:`, `a=ssrc:<n> ` (a space must follow the number), `a=x-digger-player:`.
`<host>:<port>` is the same socket SIP runs on.

SSRCs: caller `make_stream_ssrc(local_nonce, session_nonce)` = fold-xor of
`local_nonce ^ session_nonce ^ 0x5354524d` (`netsim.c:579-595`); callee an FNV-style hash of the
session nonce and its user name (`make_local_stream_ssrc()`, `netsim_sip.c:314-328`). Zero is
replaced by `0x4e53494d`. These are only *identifiers* exchanged in SDP — a port may compute them any
way it likes as long as it advertises what it sends.

SIP identifiers: a fresh Call-ID, From-tag and Via branch per call/transaction, derived from an
MD5 of random bytes plus a counter (`id_policy()`, `netsim_sip.c:572-599`;
`microsippy/platforms/posix/usipy_tm_uac.c:193-221`). `User-Agent`/`Server` =
`Digger Reloaded (<git short rev>[dirty])` (`netsim_sip_platform.c:4-19`,
`scripts/gen-build-info.sh:82-83`).

Unusual SIP behaviour to preserve for interop/tests:
- Timer L (wait for ACK to our 2xx) is 5 s, not 64·T1 (`NETSIM_SIP_TIMER_L_MS`, `netsim_sip.c:48-50,
  1487-1490`). If it expires without an ACK, the call is **kept if media is flowing** (a peer frame
  within the sync timeout; `invite_no_ack()` `netsim_sip.c:935-962`, `thread_session_live()`
  `netsim.c:2350-2361`).
- A send that cannot be resolved/sent is treated as lost and left to SIP timers
  (`socket_send_to()` always returns 0, `netsim_sip.c:433-442, 475`).
- Stray ACKs are dropped silently; BYE for an unknown dialog gets 404; other methods 501 stateless
  (`netsim_sip.c:1073-1088`).
- Max 32 concurrent SIP transactions (`NETSIM_SIP_MAX_TX`), 16-entry SIP event ring with silent
  drop when full (`netsim_sip_internal.h:18-19`, `netsim_sip.c:183-196`).

### 2.7 Clock / pacing sync
No shared clock. Two mechanisms:
- **RTT estimate** (NTP-style, from the echo fields): `rtprop = (recv_ms − (echo_local_tor + hold))
  + (echo_local_tor − echo_peer_ct)`, low-pass filtered with shift 2
  (`netsim.c:2076-2092, 1483-1501`). Used only for the retransmit interval.
- **Remote lead**: for each delivered frame, `lead_ms = (our first send of frame N) − (first receipt
  of peer's frame N)` (`netsim.c:2151-2165`). It is returned through `netsim_sync_frame()` into
  `dgstate.netsim_remote_lead_ms` (`netsim_game.c:69, 95`; `game.h:10`) and added as a phase offset
  to the frame timer's PLL in `gethrt()` (`sdl_timer.c:68-69`, `fbsd_timer.c:75`). Positive lead
  (peer is ahead of us) makes the local timer run a little faster, so the two machines converge on
  the same frame phase instead of one always waiting.

### 2.8 NAT traversal
**None.** IPv4 only (`AF_INET` hard-coded throughout `netsim_platform.c`, `netsim_sip.c`). The SDP
`c=` address is the locally bound address, or — when bound to 0.0.0.0 — the local address the OS
would route to the peer/server, found by `connect()`ing a throw-away UDP socket and reading
`getsockname()` (`netsim_sockaddr_local_for_peer()`, `netsim_platform.c:1067-1100`;
`resolve_local_host_for_target()` `netsim_sip.c:526-551`). No STUN/ICE/rport handling, no media
latching to the observed source. Works on a LAN, with public addresses or port-forwarding, or via a
SIP server that rewrites SDP/relays media. Because SIP and media share the port, a single forwarded
UDP port suffices for the listening side.

### 2.9 Desync detection and recovery
- **There is no in-protocol desync detection**: no state hash is exchanged, and there is no
  resynchronisation. NetSim relies entirely on determinism. `grep hash netsim*.c` hits only
  `netsim_debug.c`.
- Failure handling is binary: any protocol error (timeout, seq gap, window overflow, SIP error,
  socket error) ends the session; `netsim_sync_frame()` returns false, the game sets `escape` and
  zeroes both players' network controls (`netsim_game.c:86-93`) and returns to the title.
- Offline/CI detection is available: (a) `DIGGER_NETSIM_TRACE` logs per-phase hashes on each peer
  for diffing (§6); (b) either peer can record an eDRF (`/R:x.edrf`), which contains state-hash
  checkpoints, and the replay tests compare each peer against a known-good recording.

---

## 3. Interface between NetSim and the core game

### 3.1 Functions the game calls (must exist in the Rust port, real or stub)
`netsim.h:22-46`:

| Function | Caller(s) | Purpose |
|---|---|---|
| `netsim_configure(spec)` | `main.c:1077` | parse `/N`; false ⇒ "Invalid /N argument", exit 1 |
| `netsim_configured()` | `main.c:600, 651` | gate title mode + friends save |
| `netsim_sync_enabled(bool)` | `main.c:731` | start/stop threads as mode toggles |
| `netsim_title_status_get()` | `main.c:435, 449, 492, 687` | title colour |
| `netsim_pump_title_events()` | `main.c:455` | drain in-queue on title; true ⇒ redraw friends |
| `netsim_remote_start_requested()` | `main.c:482` | read-and-clear "peer called us" |
| `netsim_friend_count/selected/get/move` | `main.c:463-466, 600`, `scores.c:307-322` | friends UI (implemented in `netsim_friends.c`) |
| `netsim_friends_save()` | `main.c:601` | (`netsim_friends.h`) |
| `netsim_start_session(initiated_locally)` | `main.c:534` | blocking handshake |
| `netsim_sync_frame(frame, bits, freeze, &rbits, &rfreeze, &lead, quit_cb)` | `netsim_game.c:86` | the per-frame exchange; blocking |
| `netsim_local_player()` | `main.c:548, 550, 814-815`, `scores.c:247`, `sound.c:281`, `netsim_game.c` | 0 = caller |
| `netsim_session_nonce()` | `main.c:118` | RNG seed base |
| `netsim_session_active()` | `main.c:117`, `digger.c:273, 306`, `scores.c:248`, `netsim_game.c:70, 172` | |
| `netsim_peer_exited()` | `scores.c:248` | |
| `netsim_await_peer_hangup()` | `netsim_game.c:155` | wait ≤ 2 s for BYE |
| `netsim_stop_session()` | `netsim_game.c:156` | hang up |
| `netsim_shutdown()` | `main.c:568, 602` | join threads |
| `netsim_no_ack_seen()` | `main.c:579` | instrumentation only |

`netsim_game.h:14-22` (game-side glue in `netsim_game.c`, compiled in *all* builds incl. Emscripten):

| Function | Caller(s) |
|---|---|
| `netsim_game_frame(frame, local_freeze, local_pause, use_pause_latch, localquit, &rfreeze, &rpause)` | `digger.c:281` (from `syncframe`) |
| `netsim_remote_pause_active()` | `main.c:818, 844` |
| `netsim_quit_synced()` | `main.c:558` |
| `netsim_drain_frames()` / `netsim_draining()` | `main.c:559`, `scores.c:250` / `digger.c:268, 272` |
| `netsim_end_game_session(game_over)` | `main.c:560`, `scores.c:251` |
| `netsim_quitter()` | `edrf.c:595` |

Global state touched: `dgstate.netsim` (bool mode flag) and `dgstate.netsim_remote_lead_ms`
(`game.h:10, 13`, `game.c:14-16`).

### 3.2 Game functions NetSim calls
- `input_snapshot_primary_controls()` (`input.c:179-197`) — local bits; also clears the "was pressed"
  latches.
- `input_set_network_controls(slot, bits)` (`input.c:238-245`) — inject bits for a digger slot.
  `main.c:547-548` first calls `input_enable_network_mode()` which sets *both* slots' source to
  `INPUT_SOURCE_NETWORK` (`input.c:227-236`); thereafter `apply_network_controls()`
  (`input.c:294-…`) is the only path into the simulation — **the local player's controls also go
  through the network bits**, guaranteeing both peers consume identical values.
  `input_reset_network()` restores keyboard sources (`main.c:529, 592`).
- `checkkeyb()` and the `escape`, `pausef` globals; `getlives()`; `freezeframe()` (recursion back
  into `syncframe` for the drain loop, `netsim_game.c:176-178`).
- Logging only: `getframe()`, `gamerunning()`, `dgstate.ticks`, `digger_log_printf/ts`
  (`netsim.c:471-485`).
- `netsim_debug.c` (instrumented builds): `field_hash()`, `digger_debug_hash()`,
  `monster_debug_hash()`, `bags_debug_hash()`, `dgstate.randv/timeout/curplayer`.
- Friends: nothing from the game.

### 3.3 Per-frame and end-of-game logic in `netsim_game.c` (188 lines — port verbatim)
- `netsim_game_frame()` (`:56-108`): no-op returning true when not in an active NetSim session or
  `escape` already set; else builds local bits (keyboard, or `edrf_feedpeek()` when replaying),
  ORs PAUSE/QUIT, calls `netsim_sync_frame()`, strips the three control flags and injects both
  players' bits. On failure: `escape = true`, zero controls, return false.
- **Synchronous quit**: `syncframe()` converts a local `escape` into `localquit` and clears `escape`
  so the frame is still exchanged with the QUIT bit (`digger.c:270-275`); both sides then set
  `escape`, `quitsynced`, `quitter` on the same frame (`netsim_game.c:100-105`).
- **Drain** (`netsim_drain_frames()`, `:167-181`): after game over or a synced quit, keep
  exchanging FREEZE frames until the peer also freezes, so our last real frames can be
  retransmitted if lost.
- **Who hangs up** (`netsim_end_game_session()`, `:142-160`): after a synced quit the quitter hangs
  up (player 1 if both quit on the same frame); after a natural game over player 1 does; the other
  side waits up to 2 s for the BYE (`NETSIM_PEER_HANGUP_WAIT_MS`, `netsim.c:52, 2854-2883`) and then
  hangs up anyway. Any other ending: hang up here immediately.
- Level-done jingle: both sides exchange FREEZE frames until both jingles have finished
  (`soundlevdone_netsim()`, `sound.c:329-360`). Death music tune selection uses the *local* player's
  lives (`sound.c:281`). These are the places where audio completion feeds the frame stream — note
  it only affects freeze frames, never simulation frames.

### 3.4 Relation to DRF/eDRF recording and the state hash
- eDRF records "the controls every player pressed on every tick" plus state checkpoints and a
  `Q <slot>` quit record (`digger.txt:483-495`). Network-injected bits pass through
  `edrf_controls()` → `edrf_input()` (`input.c:308, 350-357`; `edrf.c:514-524`), so a NetSim game can
  be recorded on either peer and replays locally as an ordinary two-digger game. The quit record's
  slot comes from `netsim_quitter()` (`edrf.c:594-595`); it is omitted if the peer just vanished.
- The round seeds are written by `recputrand()` as usual (`main.c:176`), so a recording does not
  need the nonce.
- **NetSim replay (test only)**: `DIGGER_NETSIM_REPLAY=<file.edrf>` opens a two-digger eDRF
  (`edrf_netfeed_open()`, `edrf_feed.c:30-70`; header line 3 must be `M2` or `M2I<level>`), each
  peer sends *its own slot's* recorded bits instead of the keyboard (`edrf_feedpeek()`,
  `netsim_game.c:77-80`), takes the round seeds from the recording instead of the nonce
  (`main.c:166-167`), checks the bits received for both slots against the recording
  (`edrf_feed_checkinput()`, `edrf_feed.c:112-123`) and verifies the recording's state-hash
  checkpoints. A recorded quit is initiated only by the peer whose slot quit
  (`edrf_feed_quitting()`, `edrf_feed.c:125-131`; `edrf_quitting()` `edrf.c:573-584`).
- `state_hash.c` is used by eDRF checkpoints and by `netsim_debug.c`; **never sent on the wire**.
- No high scores for NetSim replays (`scores.c:242-245`).

---

## 4. Stub build

`netsim_stubs.c` (132 lines) replaces **only `netsim.c` + `netsim_rx.c`**, and only for Emscripten
(`CMakeLists.txt:232-243`; `GNUmakefile:182-186`, `ARCH=WASM`). It defines exactly the 15 (+1
instrumented) non-friend functions of `netsim.h`:

| Stub | Returns |
|---|---|
| `netsim_configure` | `false` (so `/N` prints "Invalid /N argument" and exits) |
| `netsim_configured`, `netsim_start_session`, `netsim_await_peer_hangup`, `netsim_session_active`, `netsim_peer_exited`, `netsim_pump_title_events`, `netsim_remote_start_requested`, `netsim_no_ack_seen` | `false` |
| `netsim_sync_enabled`, `netsim_stop_session`, `netsim_shutdown` | no-op |
| `netsim_sync_frame` | `false`; sets `*remote_freeze=false`, `*remote_lead_ms=0`, leaves `*remote_bits` untouched |
| `netsim_local_player` | `0` |
| `netsim_session_nonce` | `0` |
| `netsim_title_status_get` | `NETSIM_TITLE_OFF` |

Still compiled in the stub build: `netsim_game.c`, `netsim_friends.c`, `netsim_debug.c`,
`netsim_instrument.c`, `netsim_platform.c`, `netsim_sip*.c` (the latter three compile to nothing or
to their own internal stubs under `NETSIM_PLATFORM_SUPPORTED == 0`, `netsim_platform.h:11-15`,
`netsim_sip.c:1664-1784`). Since `dgstate.netsim` can never become true there, every hook is inert.

Build flags:
- `NETSIM_PLATFORM_SUPPORTED` — derived: `!__EMSCRIPTEN__ && (UNIX || _WIN32)`.
- `DIGGER_INSTRUMENTATION` — CMake option, default ON for `CMAKE_BUILD_TYPE=Debug`, OFF otherwise
  (`CMakeLists.txt:10-17, 294-295`); GNU make variable (`GNUmakefile:9-15`). Enables fault injection,
  NetSim replay, trace, test timeouts.
- `DIGGER_DEBUG` — additionally enables `DIGGER_NETSIM_PROTO_DEBUG` logging (`netsim.c:305-376`).
- `USIPY_BIGENDIAN=0|1` — required by microsippy (`CMakeLists.txt:395-402`).
- microsippy sources and `netsim_sip_platform.c` are added only for `(UNIX OR WIN32) AND NOT
  EMSCRIPTEN` (`CMakeLists.txt:338-394`); `digger_version.h` is generated for the SIP branding.

---

## 5. Platform layer (`netsim_platform.c`, 1170 lines = Win32 half + POSIX half)

API in `netsim_platform.h:65-115`:
- **Sockets**: BSD sockets / Winsock2 (`WSAStartup(2,2)`), UDP, IPv4 only. `bind` without
  `SO_REUSEADDR` (deliberately removed, commits 2463b89/508bf5f), blocking with 100 ms
  `SO_RCVTIMEO`, `sendto`/`recvfrom`, `getsockname`. `netsim_socket_open_udp` (connected variant),
  `netsim_socket_send/recv`, `set_nonblocking_socket` exist but are unused by the current code.
- **DNS**: `getaddrinfo` with `AF_INET` hints, done synchronously on the net thread for every SIP
  send and for SDP hosts (`netsim_platform.c:813-838`, `netsim_sip.c:436`) — a slow resolver stalls
  the protocol thread.
- **Threads**: pthreads / Win32 threads; mutex + condvar (`CRITICAL_SECTION` + `CONDITION_VARIABLE`
  on Windows); timed waits against an absolute ms deadline.
- **Clocks**: `CLOCK_MONOTONIC` / `QueryPerformanceCounter` in ns; deadlines are `uint64_t` ms on the
  same clock. `digger_log.c:95-100` reuses `netsim_monotonic_ns()` for log timestamps.
- **Misc**: process id (nonce entropy), `sleep_ms`, errno/WSA error classification and strings.
- microsippy needs from its platform shim: monotonic ms, sleep-until, random bytes, log sink, UA
  string (`netsim_sip_platform.c:21-35`; defaults in `microsippy/platforms/{posix,win32}`).

Everything here maps directly onto Rust `std`: `UdpSocket` (+ `set_read_timeout`), `ToSocketAddrs`,
`std::thread`, `Mutex`/`Condvar`, `Instant`, `std::process::id()`.

---

## 6. Instrumentation, debugging, tests

### Always available
- `DIGGER_NETSIM_DEBUG=1` — verbose protocol log through `digger_log_printf` (stderr; `DIGGER.log`
  on Windows). Line format `[ts] netsim: <msg> (frame=N tick=T)` (`netsim.c:454-486`);
  SIP lines `[ts] netsim-sip: …` (`netsim_sip.c:139-150`) are **unconditional**. Errors via
  `netsim_err()` are unconditional too. `DIGGER_LOG_T0=0` makes timestamps absolute monotonic so
  two peers' logs can be merged.
- The test harness greps specific log lines — if the Rust port is to reuse the script these must be
  reproduced verbatim: `session connected local_player=N` (`netsim.c:1957`),
  `slow start over … by ACK` (`netsim.c:1529-1533`),
  `session failed: retransmit timeout: frame` (`netsim.c:1675, 1404`).

### `DIGGER_DEBUG` only
- `DIGGER_NETSIM_PROTO_DEBUG` — per-packet RTT/repair/delivery lines (`netsim_proto_log`).

### `DIGGER_INSTRUMENTATION` only (`README.md:130-158`)
| Variable | Effect | Where |
|---|---|---|
| `DIGGER_NETSIM_REPLAY=<edrf>` | feed a recording over the network; exit 0 ok / 2 diverged / 3 stopped or no session / 1 unreadable | `main.c:401-422, 562-586` |
| `DIGGER_NETSIM_REPLAY_START` | this peer starts the game by itself | `main.c:478-480` |
| `DIGGER_NETSIM_REPLAY_REPEAT=n` | n games in consecutive sessions; also enables 486-busy redial for 3 s every 250 ms | `edrf_feed.c:78-94`, `netsim_sip.c:1098-1127` |
| `DIGGER_NETSIM_TX_DROP_EVERY=n` / `RX_DROP_EVERY=n` | random loss, 1 in n, by `splitmix64(seed, counter) % n == 0`; applies to SIP *and* media | `netsim_instrument.c:106-111, 215-254`, `netsim_rx.c:53-60`, `netsim_platform.c:1016` |
| `DIGGER_NETSIM_LOSS_SEED` | reproduce a loss pattern (seed is always logged) | `netsim_instrument.c:71-102` |
| `DIGGER_NETSIM_SIP_DROP_2XX=n` | drop our first n 2xx-to-INVITE | `netsim_sip.c:373-403` |
| `DIGGER_NETSIM_SIP_NO_ACK` | never send ACK | `netsim_sip.c:445-450` |
| `DIGGER_NETSIM_MUTE_AFTER_ACK` | send nothing at all after our ACK | `netsim_sip.c:461-464`, `netsim_instrument.c:30-36` |
| `DIGGER_NETSIM_EXPECT_NO_ACK` | exit 2 unless a 2xx went un-ACKed | `main.c:578-583` |
| `DIGGER_NETSIM_SIP_TIMER_L=ms` | override Timer L | `netsim_sip.c:1481-1487` |
| `DIGGER_NETSIM_SYNC_TIMEOUT=ms` | override the 1000 ms sync timeout | `netsim.c:1228-1251` |
| `DIGGER_NETSIM_TRACE` | per-phase hash line `netsim-trace: lp= frame= phase= rand= field= dig= mon= bag= levdone= alldead= timeout= cur= penalty=` at 10 points of the tick (`pre`, `post_digger`, `post_monsters`, `post_bags`, `post_tick`, `cleanup_*`) | `netsim_debug.c:24-47`, `main.c:216-260` |

With synthetic loss enabled, retry timing is automatically tightened so that the odds of losing
every copy of a frame/INVITE are ~1e-9 (`netsim_instrument.c:113-213`; hooks at `netsim.c:1307-1312,
1548-1553`, `netsim_sip.c:793-795, 1437-1440, 1488`).

### Test suite — `scripts/do-test-netsim.sh` (320 lines), run by `scripts/do-test-run.sh:142-146` when `TEST_TYPES` includes `netsim`
Two instances on localhost ("alice" listens = player 2; "bob" registers+calls = player 1), each in
its own cwd/HOME with `SDL_AUDIODRIVER=dummy SDL_VIDEODRIVER=dummy DIGGER_CI_RUN=1
DIGGER_NETSIM_DEBUG=1`, options `/Q /S:0`, a 120 s watchdog, and a port derived from an MD5 of the
platform name in 1024..9999 (`:49-51`). For each two-digger recording in `tests/data/*.edrf`
(4 of 50 have `M2` headers):
1. Full replay, three loss variants (none, RX 1/7, TX 1/5); both peers must exit 0 and print the
   same `score=… score2=… level=… ticks=…` line as `tests/results/<name>.out` (`:269-271`).
2. Recording cut at an early checkpoint with `Q 0` / `Q 1` appended: quit by player 1, by player 2,
   and by both on the same frame (`:272-280`).
3. For one recording: the quit game 50 times in a row in 50 SIP sessions (`:281-295`).
4. Never-ACKed call with a 1.5 s Timer L: the game must still complete and the callee must have
   noticed (`:296-307`).
5. Answer lost twice (caller only gets the 200 ~1.5 s later; callee's slow start must hold) (`:308-312`).
6. Caller vanishes right after its ACK: callee must fail by the sync timeout, non-zero exit (`:221-251, 313`).
On failure both logs are merged into a single timeline by timestamp (`:113-129`).

Debian ships a separate `digger-instrumentation` package with `/usr/lib/digger/digger-instrumented`
for these tests (`README.md:160-166`, `debian/control:26-28`).

---

## 7. Third-party code: microsippy

- `microsippy/` — vendored (not a submodule) copy of sobomax/microsippy, **BSD 2-Clause**, © 2020
  Maksym Sobolyev (`microsippy/LICENSE`); same author as the Digger port. Describes itself as a
  "fast and lean … SIP (RFC3261) and RTP (RFC3550) implementation" originally for ESP8266
  (`microsippy/README.md`). Design notes (`microsippy/doc/DesignNotes/README.md`, 28 lines): arena
  ("winking") allocation co-located with the message buffer; Pearson-hash header-name lookup.
- Size: ~12.2k lines in `microsippy/src`; the ~38 files Digger compiles (`CMakeLists.txt:349-384`)
  total ~9.9k lines, of which transaction manager + UA state machine + `usipy_tm_uac.c` ≈ 5.8k.
- What Digger uses: message parser/builder, URI/Via/CSeq/name-addr/auth headers, client and server
  transactions with RFC 3261 timers (T1 = 500 ms), dialog matching, a 5-state UA
  (idle/trying/dialing/connected/disconnected), REGISTER client with refresh and digest auth, and
  helpers from the **posix CLI platform directory** (`platforms/posix/usipy_tm_uac.c` is compiled on
  Windows too). The RTP part of microsippy is not used — Digger frames its own packets.
- `microsippy/src/external/mackron_md5/md5.c` — public domain / MIT-0; used for digest auth and for
  SIP id generation. Listed in the *unconditional* source list (`CMakeLists.txt:228`).
- `spinlock.c/.h` — BSD 2-Clause, © 2019 Sippy Software.

---

## 8. Size and complexity

| Unit | Lines | Port difficulty |
|---|---:|---|
| `netsim.c` (transport, queues, session FSM, `/N` parser) | 3031 | **High** — subtle implicit-ack/parity/repair logic |
| `netsim_sip.c` (+ `_registrar.c` 248, `_sdp.c` 146, `_platform.c` 44, `_internal.h` 121) | 1784 | High, but mostly because of what sits underneath it |
| microsippy (compiled subset) | ~9900 | Very high to re-implement faithfully |
| `netsim_platform.c` | 1170 | Trivial in Rust (`std`) — maybe 150 lines |
| `netsim_friends.c` | 463 | Low |
| `netsim_instrument.c` | 275 | Low |
| `netsim_game.c` | 188 | Low but must be exact |
| `netsim_stubs.c` / `netsim_rx.c` / `netsim_debug.c` | 132 / 71 / 48 | Trivial |
| Hooks in `main.c`, `digger.c`, `input.c`, `sound.c`, `scores.c`, `edrf*.c`, timers | ~250 | Must be in place from day one |

Total own code ≈ 8.2k lines + ≈ 9.9k vendored. NetSim is larger than the entire rest of the game
logic and is the least stable part of the upstream tree.

---

## 9. Recommendation for the Rust port

### Phase 1 — identical hook points, stub backend (do this with the core port)
- Define a `NetSim` trait (or a struct with a backend enum) carrying exactly the `netsim.h`
  surface in §3.1, plus a `NullNetSim` reproducing `netsim_stubs.c` (§4) value for value.
- Port **`netsim_game.c` in full now** (it is game logic, compiled even in upstream's stub build),
  and keep every call site: `syncframe`/`pauseframe`/`freezeframe`/`redrawdelay`, `seedbase()`,
  the title loop, `testpause`/`run_pause`, `soundlevdone_netsim`, `endofgame`, `edrf_quit`,
  the `netsim_remote_lead_ms` term in the frame timer, `input_enable_network_mode` /
  `input_set_network_controls` / `input_reset_network` / `input_set_updown_start`.
- Keep `dgstate.netsim` and the 6-entry mode table with `mode_available()`; with the null backend
  the NETSIM mode is simply never offered and `/N` errors out as upstream's WASM build does.
- Port `netsim_friends` only when the mode becomes selectable (it is self-contained).

### Phase 2 — a loopback/in-process backend for tests
- A backend that pairs two game instances through channels (or a plain UDP pair with a fixed peer
  address and a pre-agreed nonce, no SIP). This exercises lockstep, freeze/pause/quit sync, drain
  and the eDRF feed path, and lets the port run the *semantics* of `do-test-netsim.sh` (replay a
  two-digger eDRF on both sides, compare result lines) without any SIP.
- Port `edrf_feed.c` and the `DIGGER_NETSIM_REPLAY*` hooks behind a cargo feature
  (`instrumentation`), mirroring `DIGGER_INSTRUMENTATION`.

### Phase 3 — wire-compatible media transport over `std::net::UdpSocket`
- Hand-roll it; no crate is worth pulling in for a fixed 44-byte packet. Serialise field by field
  with `to_be_bytes` (do not transmute a `#[repr(C)]` struct).
- Port `struct netsim_thread_state` and the functions `thread_set_tx`, `thread_ack_tx`,
  `thread_accept_peer_seq`, `thread_mark_matching_peer_tx`, `thread_note_peer_frame*`,
  `thread_resend_*`, `thread_fill_send_meta`, `send_pending`, `pending_retry_ms`,
  `thread_slow_start`, `thread_store_remote_frame`, `thread_deliver_ready_frames` **line for line**
  as a pure state machine (`fn on_packet`, `fn on_local_frame`, `fn on_tick(now)`), with time
  injected. That makes it unit-testable without sockets — something upstream lacks.
- Threads: `std::thread` ×2 with `std::sync::mpsc::sync_channel(32)` (bounded, blocking — same
  back-pressure as upstream) or `Mutex<VecDeque>` + `Condvar` if the session-clear filtering of the
  out-queue (`queue_out_clear_session`) is to be kept exact. `recv_timeout` replaces timed waits.
  `tokio`/async is unnecessary and would complicate the blocking game loop.
- Instrumentation: a `drop_check(len)` hook in the send path and RX loop using the game's own
  `splitmix64`.

### Phase 4 — SIP signalling (needed for real interop with the C build)
Options, in order of preference:
1. **Hand-roll a minimal SIP UA + mini-registrar** (est. 1.5–2.5k lines of Rust). Needed subset:
   UDP only; REGISTER (client with refresh, 401/407 digest MD5 `qop=auth`; server side storing one
   Contact per user); INVITE/1xx/200/ACK/BYE; client and server transaction timers A/B/E/F/H/J/L
   with the non-standard values in §2.6; 403/404/486/488/501 responses; one dialog at a time.
   Crates that help: `md-5` (digest), `rand`/`getrandom` (ids), optionally `rsip` for
   parsing/typed headers (evaluate strictness against microsippy's output first). Full stacks
   (`rvoip`, `ezk-sip`) are async/tokio-based and far larger than the need; not recommended.
2. **Link microsippy via FFI** (`cc` crate building the vendored C, thin `unsafe` wrapper around the
   ~15 `usipy_*` entry points used by `netsim_sip.c`). BSD-2 makes this legally easy; it gives
   byte-identical SIP behaviour immediately and keeps `netsim_sip.c`'s glue portable 1:1. Good
   interim step, at the cost of a C toolchain dependency and non-"pure Rust".
3. Skip SIP: add a non-upstream "direct IP" mode (static peer address + nonce derived from a
   shared secret). Simple, but **not interoperable** with the C build.

### What must be wire-compatible to interoperate with the C build
1. The 44-byte media packet (§2.4), including PT 96, version bits, big-endian fields, repair flag
   and the exact acceptance rules (nonce, SSRC, player, source latch).
2. The reliability *behaviour* (§2.5): `tx_seq` starting at 1 and advancing once per frame, frames
   starting at 1, the 4-wide seq window and 32-wide frame window (violations make the C peer fail
   the session), `unseen_off` fitting in int8.
3. SDP attribute names and syntax (§2.6): `a=x-digger-session:` (decimal u64), `a=ssrc:<n> …`,
   `a=x-digger-player:`, `m=application <port> RTP/AVP …`, `c=IN IP4 …`; caller is player 0.
4. Standard SIP over UDP on the same socket as media, REGISTER-before-INVITE in listen mode
   (the C listener will not call or accept from an unregistered peer in the way a user expects),
   user-name based request targeting, 486 when busy.
5. Control-bit values (`0x01..0x10`, `0x20/0x40/0x80`) and their frame semantics (freeze, pause,
   synced quit, drain, who hangs up).
6. **Bit-exact simulation**: `splitmix64(session_nonce, round)` seeding and a fully deterministic
   game, *including which frames are freeze frames* (redraw delays, level-done, pause). Any
   divergence is undetected on the wire and simply desyncs the two screens. This is the real
   interoperability bar and is the same bar eDRF playback sets.
Not required to match: SSRC derivation, nonce generation, Call-ID/tag/branch formats, User-Agent,
log text (unless the upstream test script is reused), friends file (unless sharing `~/.digger.frd`).

---

## 10. Trickiest parts and open questions

**Tricky**
1. The implicit-ack scheme (§2.5) depends on both peers' `tx_seq` counters advancing in lockstep
   with the frame counter. It is undocumented, has changed recently, and has many interacting
   flags (`matched`, `peer_frame_base/count`, `send_count`, `prev_tx`, `pending_frame`). Port it
   mechanically first; do not "simplify".
2. Session boundaries: stale events from the previous session in either queue, the out-queue
   filter that preserves the *next* session's START, BYE retransmissions overtaking the next
   INVITE (hence per-call SIP ids and the test-only busy redial). The 50-games-in-a-row test exists
   because of bugs here.
3. Blocking semantics in the game loop: `netsim_sync_frame()` blocks without a timeout once a frame
   has been synced (`netsim.c:2931-2932`) and relies on the net thread to post an error within the
   sync timeout; before the first synced frame it polls `quit_requested()` every 100 ms
   (`NETSIM_QUIT_POLL_MS`). The window/SDL event pump is starved while blocked. `queue_out_put`
   also blocks when the queue is full.
4. Freeze frames driven by sound completion (`sound.c:329-360`) and the drain loop re-entering
   `freezeframe()` with `escape` temporarily cleared (`netsim_game.c:170-180`) — easy to break when
   restructuring the main loop in Rust.
5. `input_snapshot_primary_controls()` has side effects (clears key latches) and is replaced by the
   eDRF feed under instrumentation; the local player's inputs must go through the network path too.
6. SIP corner cases encoded as behaviour: keep the call when the ACK never arrives but media
   flows; callee slow start; treating unsendable SIP as loss; Timer B/L overrides.
7. The frame-timer PLL coupling via `netsim_remote_lead_ms` belongs to the timing code but only
   makes sense with NetSim; with the stub it must be 0.

**Open questions**
- Is interoperability with the C build actually a goal? If not, phases 1–3 plus a simple direct-IP
  rendezvous give the feature at perhaps 15% of the cost. If yes, pin an upstream commit: the wire
  format is still moving (this report describes commit 638125e, 2026-10-14).
- Should the Rust port keep "incoming call auto-starts the game with no prompt" and the
  unauthenticated registrar accepting any user when no peer is named? Both are faithful but are
  questionable behaviour on an open network.
- IPv6 and NAT traversal are absent upstream; adding them breaks nothing on the wire only if the
  SDP stays `IN IP4`.
- The `NETSIM_OUT_HELLO` type, the "control SSRC", `netsim_socket_open_udp`/`send`/`recv` and
  `set_nonblocking_socket` appear to be dead code left from an earlier pre-SIP protocol; safe to
  omit.
- Big-endian nonce encoding bug (§2.4) — report upstream or just emit canonical big-endian.
- Whether to reuse `scripts/do-test-netsim.sh` against the Rust binary: it requires the same
  command-line syntax, env vars, exit codes, `score=` result line and three specific log lines.
- WASM target: upstream stubs NetSim out; a Rust/WASM build would need the same (no UDP).
