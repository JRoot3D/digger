//! Building the hash's plain field values from the simulation's read-only
//! [`StateView`] (design D5).
//!
//! Only the live state of the player currently playing is read: the field
//! in play, the bags in play and the digger records, never the saved field
//! or saved bags of the other alternating player. Every enum becomes its
//! wire code through [`codes`](super::codes).

use crate::{BagId, DiggerId, MonsterSlot, ScoreId, StateView};

use super::codes;
use super::{
    BagBlockFields, BagHashFields, BagSharedFields, DiggerBlockFields, DiggerHashFields,
    DiggerSharedFields, GauntletHashFields, MonsterBlockFields, MonsterBodyFields,
    MonsterHashFields, MonsterScheduleFields, StateHashFields,
};

impl StateHashFields {
    /// The hashed values of the state `view` shows.
    pub fn from_view(view: StateView<'_>) -> Self {
        let mut playfield = [0; super::CELLS];
        for (out, cell) in playfield.iter_mut().zip(view.field().cells()) {
            *out = cell.bits();
        }
        Self {
            playfield,
            digger: digger_block(view),
            monster: monster_block(view),
            bag: bag_block(view),
            rng_state: view.rng_state(),
            scores: ScoreId::ALL.map(|id| view.scores().total(id)),
            gauntlet: view.gauntlet_clock().map(|clock| GauntletHashFields {
                clock,
                timed_out: view.timed_out(),
            }),
        }
    }
}

/// The digger block.
fn digger_block(view: StateView<'_>) -> DiggerBlockFields {
    DiggerBlockFields {
        shared: DiggerSharedFields {
            bonus_start_time_left: view.bonus_start_time_left(),
            bonus_time_left: view.bonus_time_left(),
            emerald_mask: codes::emerald_mask(view.current_player()),
            bonus_visible: view.bonus_visible(),
            bonus_mode: view.bonus_mode(),
            diggers_visible: view.diggers_visible(),
        },
        emeralds: *view.emeralds().cells(),
        slots: DiggerId::ALL.map(|id| digger_fields(view, id)),
    }
}

/// One digger record.
fn digger_fields(view: StateView<'_>, id: DiggerId) -> DiggerHashFields {
    let d = view.digger(id);
    DiggerHashFields {
        h: codes::small(d.located.cell.h()),
        v: codes::small(d.located.cell.v()),
        rx: codes::small(d.located.rem_x),
        ry: codes::small(d.located.rem_y),
        moved: codes::optional_direction(d.moving),
        bag_time: codes::counter(d.bag_time),
        recharge_time: codes::counter(d.recharge_time),
        death_stage: codes::death_stage(d.death),
        death_bag: codes::bag(d.death_bag),
        death_frame: codes::small(d.death_frame),
        death_timer: codes::counter(d.death_timer),
        emerald_window: codes::small(d.emerald_window),
        emerald_run: codes::small(d.emerald_run),
        eat_multiplier: d.eat_multiplier,
        lives: codes::low_16(d.lives),
        invincibility: codes::counter(d.invincibility),
        not_firing: d.not_firing,
        // Quirk P10: the original keeps `firepressed` and `levdone` in each
        // digger record but never sets them; they hash as constant 0.
        fire_pressed: false,
        dead: d.dead,
        level_done: false,
        invincible: d.invincible,
        x: codes::coordinate(d.position.x),
        y: codes::coordinate(d.position.y),
        facing: codes::optional_direction(d.facing),
        alive: d.alive,
        fireball_x: codes::coordinate(d.fireball.position.x),
        fireball_y: codes::coordinate(d.fireball.position.y),
        fireball_direction: codes::direction(d.fireball.direction),
        fireball_stage: codes::fireball_stage(d.fireball.stage),
    }
}

/// The monster block.
fn monster_block(view: StateView<'_>) -> MonsterBlockFields {
    let m = view.monsters();
    MonsterBlockFields {
        schedule: MonsterScheduleFields {
            next_monster: codes::counter(m.spawned()),
            total_monsters: codes::counter(m.total()),
            max_on_screen: codes::small(m.max_on_screen()),
            next_spawn_time: codes::counter(m.spawn_timer()),
            spawn_gap: codes::counter(m.spawn_gap()),
            next_chase: codes::small(m.next_chase()),
            bonus_pending: m.bonus_pending(),
        },
        slots: MonsterSlot::ALL.map(|slot| monster_fields(view, slot)),
    }
}

/// One monster slot.
fn monster_fields(view: StateView<'_>, slot: MonsterSlot) -> MonsterHashFields {
    let m = view.monster(slot);
    MonsterHashFields {
        h: codes::small(m.located.cell.h()),
        v: codes::small(m.located.cell.v()),
        xr: codes::small(m.located.rem_x),
        yr: codes::small(m.located.rem_y),
        direction: codes::optional_direction(m.direction),
        delay: codes::counter(m.delay),
        counter: codes::counter(m.counter),
        death: codes::monster_death(m.death),
        death_bag: codes::bag(m.death_bag),
        death_delay: codes::small(m.death_delay),
        standstill: codes::small(m.standstill),
        chase: codes::digger(m.chase),
        in_use: m.occupied,
        // Quirk P10: the body exists from the spawn until the next round's
        // monster reset; a killed monster keeps it (design D5).
        body: m.spawned.then(|| MonsterBodyFields {
            x: codes::coordinate(m.position.x),
            y: codes::coordinate(m.position.y),
            facing: codes::direction(m.facing),
            alive: m.alive,
            nobbin: m.nobbin,
        }),
    }
}

/// The bag block.
fn bag_block(view: StateView<'_>) -> BagBlockFields {
    BagBlockFields {
        shared: BagSharedFields {
            push_count: codes::small(view.push_count()),
            gold_time: codes::counter(view.gold_time()),
        },
        slots: BagId::ALL.map(|id| bag_fields(view, id)),
    }
}

/// One bag slot.
fn bag_fields(view: StateView<'_>, id: BagId) -> BagHashFields {
    let b = view.bag(id);
    BagHashFields {
        x: codes::coordinate(b.position.x),
        y: codes::coordinate(b.position.y),
        h: codes::small(b.located.cell.h()),
        v: codes::small(b.located.cell.v()),
        xr: codes::small(b.located.rem_x),
        yr: codes::small(b.located.rem_y),
        direction: codes::optional_direction(b.direction),
        wobble: codes::small(b.wobble),
        gold_age: codes::counter(b.gold_age),
        cells_fallen: codes::small(b.cells_fallen),
        wobbling: b.wobbling,
        unfallen: b.unfallen,
        exists: b.exists,
    }
}
