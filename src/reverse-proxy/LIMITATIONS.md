# Legacy translation limitations

Current lossy mappings, fallbacks and compatibility workarounds. These describe
implementation behavior, not necessarily unavoidable protocol limits.

## Snapshots

- libtw2 limits snapshots to 64 KiB. The proxy does not guarantee that translated
  snapshots fit; exceeding the limit fails snapshot construction. The event cap
  reduces event volume but does not bound total snapshot size.

## Events

- Death effect with an unmapped owner uses player 0 (the local player).
- Kill with an absent/unmapped killer uses the victim, appearing as a suicide;
  unmapped victims are skipped. Kill `mode_special` is always 0.
- Puller sounds use shotgun sounds; laser pickup uses shotgun pickup sound.
  Flag grabs always use `CtfGrabPl`, regardless of team.
- Race finishes become plain chat containing only the finish time.
- Unsupported sounds/effects are silently dropped.
- Pending effects/positional sounds are capped at 256 per client; oldest entries
  are dropped on overflow.

## Input

- Weapon slot 3 selects Puller when the physics group is `ddnet`, otherwise
  Shotgun; the two cannot be selected independently through that slot.
- Weapon selection uses the previous input's requested weapon to match DDNet's
  direct-input ordering.
- Input method is always reported as mouse/keyboard; only menu, chat, scoreboard
  and hook-collision-line flags are translated.
- Jump/hook edges lost between received inputs cannot be recovered. Fire/weapon
  counters cannot distinguish a full 64-step wrap from no change.
- Timing assumes 50 ticks per second.

## Players and characters

- At most 64 mapped players, including IDs retained for 150 ticks after leaving.
  Exhaustion fails snapshot construction.
- Country is always -1, latency and auth level are 0. Names, clans and skin names
  are truncated to legacy field sizes; skins are referenced by name only.
- Puller is represented as Shotgun. Local health, armor and ammo are capped at 10;
  absent/unlimited ammo becomes 10. Other players' values are 0.
- Character player flags, ninja activation tick and freeze start are 0;
  tune-zone override is 0. Only explicitly mapped buffs/debuffs are represented.

## Spectators and stages

- Only one stage's world is rendered; other stages contribute player info only.
  If no local/followed stage is found, the first stage is used.
- Following multiple characters collapses to one target.

## Projectiles and lasers

- Vanilla projectiles are rebased one tick backwards on each snapshot so short-lived
  pellets remain visible during legacy interpolation. This uses global tuning.
- Projectiles and lasers use basic legacy objects; extended metadata such as
  ownership and special behavior is not transmitted.

## Pickups and flags

- Weapon shields and ninja shields appear as armor pickups.
- Only the first red and first blue flag are represented.

## Match state and tuning

- Warmup and round count are 0; current round is 1. Time limits round up to minutes.
- Ground elasticity is forced to 0. Tuning values round to hundredths;
  fields without a legacy mapping are omitted.

## Votes

- Categories become numbered text labels; label text is truncated to 48 bytes.
  Options beyond 4096 are dropped.
- Vote descriptions/reasons are truncated to 64 bytes, remaining time to 1–60
  whole seconds, and yes/no/total counts to 64 each.

## Chat

- An unmapped sender becomes server sender -1. Messages are truncated to 512
  bytes and NUL characters removed.

## Server browser

- UDP supports original `getinfo` only: first 16 clients and counts capped at 16.
- Country is -1, non-integer scores become 0, and every listed client is marked
  as a player. HTTP registration also treats every listed client as a player.
- Master-server challenges are unsupported.

## Protocol and authentication

- DDNet 0.6 only; arbitrary Wasm game modules are rejected.
- Connections use guest certificates. Native account authentication and legacy
  RCON are unavailable; account-only backends remain inaccessible.
