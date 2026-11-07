# Abilities and Effects

Phase 76: a small version of a gameplay ability system, over the attributes and tags every Instance has. Code: `crates/script/src/ability.rs` (state and Lua), `crates/core/src/ability.rs` (`AbilitySlot`), `crates/ui/src/ability_bar.rs`. The example is `examples/hello-abilities` (Q casts a Fireball, R heals, the yellow pad stuns), with game tests in its `tests/`.

## Ability

An `Ability` in a Model or a Player. Its **holders** are that parent, plus that Player's character or that character's Player.

| Property | Does |
|---|---|
| `Cooldown` | Seconds after activating before it can activate again. |
| `CostAttribute`, `CostAmount` | Activating spends `CostAmount` of the first holder's number attribute `CostAttribute` (Mana 20). Less than that blocks it. Empty costs nothing. |
| `ActivationKey` | The input map action that activates it for the player who holds it. Empty: scripts only. |
| `BlockedByTags` | Comma-separated tags; any holder having one blocks it (`Stunned`). |
| `GrantsTags`, `Duration` | While active (`Duration` seconds after activating), the character (else the parent) has these tags. Duration 0 ends at once. |
| `Icon` | The ability bar's picture; empty shows its Name. |
| `CooldownLeft`, `Active` | Read-only, kept by the server. |

- `CanActivate()` returns `true`, or `false` and why ("it's cooling down (2.1 s left)", "its holder has the Stunned tag", "its holder hasn't 20 Mana", "it's already active").
- `Activate()` in a Script activates it if it can and returns whether it did. In a LocalScript it sends a request to the server (if it passes the local check) and returns whether it asked.
- `Cancel()` (Scripts) ends it early: tags off, `Ended(true)`. The cooldown carries on.
- Events, on both sides of the device that runs the server: `Activated(player)` (nil for a Script), `Ended(cancelled)`, `CooldownChanged(left)` as a cooldown starts (`Cooldown`) and ends (0). A script on `Activated` does what the ability does (the damage); cost, cooldown and tags need no script.
- A tag an Ability or Effect gives is taken off when its last giver ends; a tag the holder had from elsewhere is left on.

## Effect

An `Effect` is active while its Parent is a Model in the Workspace. Keep templates elsewhere (ReplicatedStorage, ServerStorage, inside an Ability).

| Property | Does |
|---|---|
| `Attribute` | What it changes on its Model: a number property of its Health (`Health`, `MaxHealth`, `RegenRate`), else of its character controller (`WalkSpeed`, `JumpHeight`: the Humanoid's fields), else the Model's number attribute (a missing one counts as 0). Empty changes nothing (a tag-only effect, such as a stun). |
| `Operation` | `Enum.EffectOperation.Add` or `Multiply`; each stack does it again. Adding below 0 to `Health` is `TakeDamage` (source: the Effect; Invulnerable and ForceFields apply), above 0 is `Heal`. |
| `Amount` | What it adds or multiplies by. |
| `Duration` | 0: instant, changes once and is destroyed. Above 0: lasts that long, then is destroyed. Below 0: until removed. |
| `Period` | With a Duration: above 0 changes again every `Period` seconds, for good (first change after one period; a 10 s poison with Period 1 hits 10 times). 0 makes one lasting change, undone (subtracted, or divided by the factor) when it ends or is removed. |
| `MaxStacks` | `ApplyEffect` again adds a stack up to this, and starts the Duration over. |
| `GrantsTags` | Tags its Model has while it's active. |
| `Stacks`, `TimeLeft` | Read-only. |

`model:ApplyEffect(effect)` (Scripts) copies the template onto the Model, or adds a stack to the copy of the same Name already there; it returns that Effect, or nil for an instant one. Effects placed by other means (dragged in, `Parent` set) start at the next abilities step. Removing an Effect (`Destroy`, or moving it out of its Model) stops it at the next step: a lasting change is undone, periodic changes already made stay.

## Data tables

`DataService:CreateAbility(table, key)` and `CreateEffect(table, key)` make one from a row ([../project/data-tables.md](../project/data-tables.md)): named after the key, with no Parent; a column named after a property sets it (an enum column's option names the item), and other columns become attributes (`fireball:GetAttribute("Damage")`).

## Steps and the network

- `ScriptRuntime::step_abilities(world, seconds)` runs in frame step 1 after `step_tools`, on every device: the local player's Abilities whose action was just pressed (once per press; nothing while a Sequence locks input) ask to activate. On a host or single player the request is served at once; a device following a server sends it (`MessageKind::Activate`, `WireKind::Activate`, protocol 7) and predicts the cooldown so the ability bar sweeps the same frame.
- The server serves requests only for an Ability the asking player holds, checking cost, cooldown and tags again; a modified client that clears its own cooldown still gets one activation per cooldown (`net/src/ability_tests.rs`). The host accepts `Activate` only for an Ability outside the server-only services.
- `step_health` (step 4, server only) then counts cooldowns, active abilities and Effects down. `CooldownLeft`, `Stacks`, `TimeLeft` and the Effects themselves replicate as ordinary properties and Instances.
- The **ability bar** (`ScriptRuntime::ability_bar` → `ui::ability_bar_items`) sits at the bottom right in the player and editor Play: one slot per held Ability with an `ActivationKey`, its icon or name, its key, and while cooling down a dark sweep from the top that shrinks with the cooldown and the seconds left. `Ability` rules style slots, `Ability::key` the key.
- Editor: the ribbon's Gameplay ▸ Character menu offers Ability and Effect beside Health and Damage Zone, with their own icons.

Tests: `script/src/ability_tests.rs`, `net/src/ability_tests.rs`, `ui/src/ability_bar.rs`, and `examples/hello-abilities/tests/abilities.spec.lua`.
