// IndustrialCraft 2 for Horizon: an EU network (cables), a generator, a solar panel, a BatBox, an electric furnace and a
// macerator. Energy (EU) lives in each machine's block entity (the `eu` number in its data), and a server tick moves it
// along the cables between producers, consumers and batteries. No `//!` comments: module payloads can't hold inner
// attributes.

use super::Game;
use horizon_client::containers::{ModBlockEntity, ModContainer};
use horizon_client::items::ItemStack;
use horizon_world::nbt::{Compound, JavaString, Tag};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

pub const ID: &str = "ic2";

type Pos = [i32; 3];

const DIRS: [Pos; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// EU a burning generator makes each tick.
const GENERATOR_RATE: i64 = 10;

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Producer,
    Consumer,
    Battery,
}

#[derive(Default)]
struct Server {
    tick: u64,
    /// Per dimension: the machines that ticked lately (their block id and the tick they last reported).
    devices: HashMap<String, HashMap<Pos, (String, u64)>>,
    /// Block property changes to make after the block entities have ticked: (dimension, position, key, value).
    props: Vec<(String, Pos, &'static str, &'static str)>,
}

thread_local! {
    static SERVER: RefCell<Server> = RefCell::new(Server::default());
}

fn role(block: &str) -> Option<Role> {
    match block {
        "ic2:generator" | "ic2:solar_panel" | "ic2:creative_generator" => Some(Role::Producer),
        "ic2:electric_furnace" | "ic2:macerator" => Some(Role::Consumer),
        "ic2:batbox" => Some(Role::Battery),
        _ => None,
    }
}

fn is_machine(block: &str) -> bool {
    role(block).is_some()
}

/// The most EU a machine holds.
fn capacity(block: &str) -> i64 {
    match block {
        "ic2:batbox" => 40_000,
        "ic2:creative_generator" => 1_000,
        "ic2:generator" => 100,
        "ic2:solar_panel" => 20,
        _ => 40,
    }
}

/// The most EU a cable passes in a tick (the weakest cable of a network limits the whole network).
fn cable_limit(block: &str) -> Option<i64> {
    match block {
        "ic2:copper_cable" | "ic2:insulated_copper_cable" => Some(32),
        "ic2:gold_cable" => Some(128),
        "ic2:hv_cable" => Some(512),
        _ => None,
    }
}

/// EU a tick and ticks a job takes: IC2's electric furnace (3 EU/t, 130 ticks) and macerator (2 EU/t, 300 ticks).
fn spec(block: &str) -> Option<(i64, i64)> {
    match block {
        "ic2:electric_furnace" => Some((3, 130)),
        "ic2:macerator" => Some((2, 300)),
        _ => None,
    }
}

fn has_lit(block: &str) -> bool {
    matches!(block, "ic2:generator" | "ic2:electric_furnace" | "ic2:macerator")
}

/// The macerator's recipes: what it crushes, what comes out and how many.
fn macerate(item: &str) -> Option<(&'static str, u32)> {
    Some(match item {
        "minecraft:iron_ore" | "minecraft:deepslate_iron_ore" | "minecraft:raw_iron" => ("ic2:iron_dust", 2),
        "minecraft:gold_ore" | "minecraft:deepslate_gold_ore" | "minecraft:nether_gold_ore" | "minecraft:raw_gold" => ("ic2:gold_dust", 2),
        "minecraft:copper_ore" | "minecraft:deepslate_copper_ore" | "minecraft:raw_copper" => ("ic2:copper_dust", 2),
        "minecraft:coal" => ("ic2:coal_dust", 1),
        "minecraft:cobblestone" => ("minecraft:sand", 1),
        "minecraft:sandstone" => ("minecraft:sand", 4),
        "minecraft:bone" => ("minecraft:bone_meal", 6),
        "minecraft:blaze_rod" => ("minecraft:blaze_powder", 4),
        _ => return None,
    })
}

fn java(s: &str) -> JavaString {
    JavaString::from(s)
}

fn nbt_i64(nbt: &Compound, key: &str) -> i64 {
    match nbt.get(&java(key)) {
        Some(Tag::Long(v)) => *v,
        Some(Tag::Int(v)) => i64::from(*v),
        _ => 0,
    }
}

fn set_nbt_i64(nbt: &mut Compound, key: &str, value: i64) {
    nbt.insert(java(key), Tag::Long(value));
}

fn eu_of(entity: &ModBlockEntity) -> i64 {
    nbt_i64(&entity.nbt, "eu")
}

fn set_eu(entity: &mut ModBlockEntity, value: i64) {
    set_nbt_i64(&mut entity.nbt, "eu", value);
}

fn max_stack(game: &Game<'_>, id: &str) -> u32 {
    game.res.items().get(id).map_or(64, |i| i.max_stack.max(1))
}

fn fuel_ticks(game: &Game<'_>, item: &str) -> Option<i64> {
    // The item's furnace burn time, as Horizon's furnace works it out.
    let ticks = game.res.providers().burn_time(game.res.items(), &ItemStack::new(item, 1), false);
    (ticks > 0).then_some(i64::from(ticks))
}

fn sunny(game: &Game<'_>) -> bool {
    let day = game.native_day_time().rem_euclid(24000);
    (0..12500).contains(&day)
}

// ---------------------------------------------------------------- block entities

/// `native_block_entity`: every machine gets a block entity of its own id.
pub fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if entity.is_none() && is_machine(block) {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

pub fn block_entity_ticks(block: &str, ticks: &mut bool) {
    if is_machine(block) {
        *ticks = true;
    }
}

/// The generator's fuel slot, and the furnace's and macerator's input and output (open to hoppers and pipes).
pub fn block_entity_container(block: &str, out: &mut Option<ModContainer>) {
    *out = match block {
        "ic2:generator" => Some(ModContainer {
            slots: 1,
            faces: std::array::from_fn(|_| vec![0]),
            inputs: vec![0],
            outputs: Vec::new(),
            menu: Some(block.to_owned()),
        }),
        "ic2:electric_furnace" | "ic2:macerator" => Some(ModContainer {
            slots: 2,
            faces: std::array::from_fn(|_| vec![0, 1]),
            inputs: vec![0],
            outputs: vec![1],
            menu: Some(block.to_owned()),
        }),
        _ => return,
    };
}

/// `native_block_entity_tick`: a machine reports in and does its own work.
pub fn block_entity_tick(game: &mut Game<'_>, pos: Pos, entity: &mut ModBlockEntity) {
    let id = entity.id.to_string();
    if !is_machine(&id) {
        return;
    }
    let slots = match id.as_str() {
        "ic2:generator" => 1,
        "ic2:electric_furnace" | "ic2:macerator" => 2,
        _ => 0,
    };
    if entity.items.len() < slots {
        entity.items.resize(slots, None);
    }
    let dim = game.dim.id().to_owned();
    SERVER.with_borrow_mut(|s| {
        let tick = s.tick;
        s.devices.entry(dim.clone()).or_default().insert(pos, (id.clone(), tick));
    });
    let cap = capacity(&id);
    let mut eu = eu_of(entity);
    let mut lit = false;
    match id.as_str() {
        "ic2:generator" => lit = generator_tick(game, entity, &mut eu, cap),
        "ic2:solar_panel" => {
            if sunny(game) && eu < cap {
                eu += 1;
            }
        }
        "ic2:creative_generator" => eu = cap,
        "ic2:electric_furnace" | "ic2:macerator" => lit = process_tick(game, entity, &id, &mut eu),
        _ => {}
    }
    set_eu(entity, eu.clamp(0, cap));
    if has_lit(&id) {
        // What the machine's window shows: how far along it is (percent), and its EU.
        let percent = match spec(&id) {
            Some((_, total)) => nbt_i64(&entity.nbt, "progress") * 100 / total.max(1),
            None => nbt_i64(&entity.nbt, "burn") * 100 / nbt_i64(&entity.nbt, "burn_total").max(1),
        };
        entity.data.resize(2, 0);
        entity.data[0] = percent.clamp(0, 100) as i32;
        entity.data[1] = eu.clamp(0, cap) as i32;
        let value = if lit { "true" } else { "false" };
        SERVER.with_borrow_mut(|s| s.props.push((dim, pos, "lit", value)));
    }
}

/// Burns the fuel in the slot, a tick of burning for each GENERATOR_RATE EU made (it waits while its buffer is full).
fn generator_tick(game: &Game<'_>, entity: &mut ModBlockEntity, eu: &mut i64, cap: i64) -> bool {
    let mut burn = nbt_i64(&entity.nbt, "burn");
    if burn <= 0 && *eu + GENERATOR_RATE <= cap {
        let fuel = entity.items.first().cloned().flatten();
        if let Some(f) = fuel
            && let Some(ticks) = fuel_ticks(game, &f.item)
        {
            if let Some(slot) = entity.items.first_mut()
                && let Some(s) = slot.as_mut()
            {
                s.count -= 1;
                if s.count == 0 {
                    *slot = None;
                }
            }
            burn = ticks;
            set_nbt_i64(&mut entity.nbt, "burn_total", ticks);
        }
    }
    let mut running = false;
    if burn > 0 && *eu + GENERATOR_RATE <= cap {
        *eu += GENERATOR_RATE;
        burn -= 1;
        running = true;
    }
    set_nbt_i64(&mut entity.nbt, "burn", burn);
    running
}

/// What a machine makes of an item (none: it doesn't take it).
fn recipe(game: &Game<'_>, block: &str, stack: &ItemStack) -> Option<ItemStack> {
    match block {
        "ic2:electric_furnace" => game.res.recipes().cook("minecraft:smelting", stack).map(|r| r.result.clone()),
        "ic2:macerator" => macerate(&stack.item).map(|(id, n)| ItemStack::new(id, n)),
        _ => None,
    }
}

/// Whether the output slot has room for another result.
fn can_hold(game: &Game<'_>, entity: &ModBlockEntity, out: &ItemStack) -> bool {
    match entity.items.get(1) {
        Some(None) => true,
        Some(Some(held)) => &*held.item == &*out.item && held.count + out.count <= max_stack(game, &out.item),
        None => false,
    }
}

/// The electric furnace and the macerator: while powered, a job's progress goes up one a tick; done, the input becomes the
/// result. Returns whether it worked this tick.
fn process_tick(game: &Game<'_>, entity: &mut ModBlockEntity, block: &str, eu: &mut i64) -> bool {
    let Some((cost, total)) = spec(block) else { return false };
    let input = entity.items.first().cloned().flatten();
    let result = input.as_ref().and_then(|stack| recipe(game, block, stack));
    let mut progress = nbt_i64(&entity.nbt, "progress");
    let mut running = false;
    match result {
        Some(out) if can_hold(game, entity, &out) => {
            if *eu >= cost {
                *eu -= cost;
                progress += 1;
                running = true;
                if progress >= total {
                    progress = 0;
                    if let Some(slot) = entity.items.first_mut()
                        && let Some(s) = slot.as_mut()
                    {
                        s.count -= 1;
                        if s.count == 0 {
                            *slot = None;
                        }
                    }
                    match entity.items.get_mut(1) {
                        Some(slot @ None) => *slot = Some(out),
                        Some(Some(held)) => held.count += out.count,
                        _ => {}
                    }
                }
            }
        }
        _ => progress = 0,
    }
    set_nbt_i64(&mut entity.nbt, "progress", progress);
    running
}

// ---------------------------------------------------------------- the EU network

/// `native_server_tick` (the end of the tick): each dimension's machines are grouped into networks (through cables, or
/// side by side), and each network moves its EU.
pub fn server_tick(game: &mut Game<'_>) {
    let back = game.dim;
    for dim in game.levels_to_tick() {
        if game.enter_level(None, dim).is_ok() {
            level_step(game);
        }
    }
    let _ = game.enter_level(None, back);
    SERVER.with_borrow_mut(|s| s.tick += 1);
}

fn level_step(game: &mut Game<'_>) {
    let dim = game.dim.id().to_owned();
    let (devices, props) = SERVER.with_borrow_mut(|s| {
        let tick = s.tick;
        let devices: Vec<(Pos, String)> = match s.devices.get_mut(&dim) {
            Some(map) => {
                map.retain(|_, (_, seen)| tick.saturating_sub(*seen) <= 2);
                map.iter().map(|(p, (id, _))| (*p, id.clone())).collect()
            }
            None => Vec::new(),
        };
        let mut mine = Vec::new();
        let mut rest = Vec::new();
        for change in std::mem::take(&mut s.props) {
            if change.0 == dim {
                mine.push(change);
            } else {
                rest.push(change);
            }
        }
        s.props = rest;
        (devices, mine)
    });
    // Block property changes wait for here, outside the block entities' own ticks.
    for (_, pos, key, value) in props {
        if game.native_block_property(pos, key).is_some_and(|v| v != value) {
            game.native_set_block_property_with(pos, key, value, 3);
        }
    }
    if devices.len() > 1 {
        network(game, devices);
    }
}

fn network(game: &mut Game<'_>, devices: Vec<(Pos, String)>) {
    let ids: HashMap<Pos, String> = devices.into_iter().collect();
    let mut cables: HashMap<Pos, Option<i64>> = HashMap::new();
    let mut done: HashSet<Pos> = HashSet::new();
    let starts: Vec<Pos> = ids.keys().copied().collect();
    for start in starts {
        if !done.insert(start) {
            continue;
        }
        let mut members = vec![start];
        let mut limit = i64::MAX;
        let mut wires: HashSet<Pos> = HashSet::new();
        let mut queue = VecDeque::from([start]);
        while let Some(p) = queue.pop_front() {
            for d in DIRS {
                let q = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
                if ids.contains_key(&q) {
                    if done.insert(q) {
                        members.push(q);
                        queue.push_back(q);
                    }
                } else if !wires.contains(&q) {
                    let cap = *cables.entry(q).or_insert_with(|| cable_limit(&game.native_block(q)));
                    if let Some(c) = cap {
                        wires.insert(q);
                        limit = limit.min(c);
                        queue.push_back(q);
                    }
                }
            }
        }
        if members.len() > 1 {
            distribute(game, &ids, &members, limit);
        }
    }
}

/// Moves `amount` over the entries of `list` (each holds at most the number beside it), noting the change of each.
fn spread(list: &mut [(Pos, i64)], mut amount: i64, sign: i64, deltas: &mut HashMap<Pos, i64>) {
    for entry in list.iter_mut() {
        if amount <= 0 {
            break;
        }
        let part = entry.1.min(amount);
        entry.1 -= part;
        amount -= part;
        *deltas.entry(entry.0).or_insert(0) += sign * part;
    }
}

fn total(list: &[(Pos, i64)]) -> i64 {
    list.iter().map(|e| e.1).sum()
}

/// One network's tick: producers feed consumers, what is left charges the batteries, and the batteries make up what the
/// consumers still lack. No more than the weakest cable's limit moves in all.
fn distribute(game: &mut Game<'_>, ids: &HashMap<Pos, String>, members: &[Pos], limit: i64) {
    let mut producers: Vec<(Pos, i64)> = Vec::new();
    let mut consumers: Vec<(Pos, i64)> = Vec::new();
    let mut stored: Vec<(Pos, i64)> = Vec::new();
    let mut room: Vec<(Pos, i64)> = Vec::new();
    for &p in members {
        let id = &ids[&p];
        let eu = game.native_with_mod_block_entity(p, |e| eu_of(e)).unwrap_or(0);
        let cap = capacity(id);
        match role(id) {
            Some(Role::Producer) => {
                if eu > 0 {
                    producers.push((p, eu));
                }
            }
            Some(Role::Consumer) => {
                if cap - eu > 0 {
                    consumers.push((p, cap - eu));
                }
            }
            Some(Role::Battery) => {
                if eu > 0 {
                    stored.push((p, eu));
                }
                if cap - eu > 0 {
                    room.push((p, cap - eu));
                }
            }
            None => {}
        }
    }
    let mut budget = limit;
    let mut deltas: HashMap<Pos, i64> = HashMap::new();
    let direct = total(&producers).min(total(&consumers)).min(budget);
    if direct > 0 {
        spread(&mut producers, direct, -1, &mut deltas);
        spread(&mut consumers, direct, 1, &mut deltas);
        budget -= direct;
    }
    let charge = total(&producers).min(total(&room)).min(budget);
    if charge > 0 {
        spread(&mut producers, charge, -1, &mut deltas);
        spread(&mut room, charge, 1, &mut deltas);
        budget -= charge;
    }
    let draw = total(&consumers).min(total(&stored)).min(budget);
    if draw > 0 {
        spread(&mut stored, draw, -1, &mut deltas);
        spread(&mut consumers, draw, 1, &mut deltas);
    }
    for (p, delta) in deltas {
        if delta == 0 {
            continue;
        }
        let cap = capacity(&ids[&p]);
        game.native_with_mod_block_entity(p, |e| {
            let value = (eu_of(e) + delta).clamp(0, cap);
            set_eu(e, value);
        });
    }
}

// ---------------------------------------------------------------- windows

/// The machines that open a window (the generator's, the electric furnace's and the macerator's).
fn has_window(block: &str) -> bool {
    matches!(block, "ic2:generator" | "ic2:electric_furnace" | "ic2:macerator")
}

thread_local! {
    /// The numbers the open window was last drawn with (so a tooltip can read them).
    static SCREEN_DATA: RefCell<Vec<i32>> = RefCell::new(Vec::new());
}

/// `native_screen`: where each slot sits in the window (and the player's inventory under it).
pub fn screen(_game: &Game<'_>, block: &str, out: &mut Option<super::native_hooks::NativeScreen>) {
    use super::native_hooks::NativeScreen;
    let slots = match block {
        "ic2:generator" => vec![Some([80.0, 40.0])],
        "ic2:electric_furnace" | "ic2:macerator" => vec![Some([56.0, 17.0]), Some([116.0, 35.0])],
        _ => return,
    };
    *out = Some(NativeScreen {
        width: 176.0,
        height: 166.0,
        slots,
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: false,
        own_background: false,
    });
}

/// `native_screen_draw`: the window's picture, then the energy bar and the flame or arrow over it.
pub fn screen_draw(game: &Game<'_>, block: &str, data: &[i32], batch: &mut horizon_client::hud::HudBatch, origin: [f32; 2]) {
    if !has_window(block) {
        return;
    }
    SCREEN_DATA.with_borrow_mut(|d| *d = data.to_vec());
    let [ox, oy] = origin;
    let sheet = if block == "ic2:generator" { "ic2:gui/generator" } else { "ic2:gui/machine" };
    game.native_sprite(batch, sheet, [0.0, 0.0, 176.0, 166.0], [ox, oy, 176.0, 166.0]);
    let percent = data.first().copied().unwrap_or(0).clamp(0, 100);
    let eu = data.get(1).copied().unwrap_or(0).max(0);
    let cap = capacity(block) as i32;
    // The energy bar: 8 by 40, filling from the foot.
    let h = (40 * eu / cap.max(1)).clamp(0, 40) as f32;
    if h > 0.0 {
        game.native_sprite(batch, sheet, [176.0, 31.0 + 40.0 - h, 8.0, h], [ox + 34.0, oy + 22.0 + 40.0 - h, 8.0, h]);
    }
    if block == "ic2:generator" {
        // The flame burning down over the fuel slot.
        let f = (14 * percent / 100) as f32;
        if f > 0.0 {
            game.native_sprite(batch, sheet, [176.0, 14.0 - f, 14.0, f], [ox + 81.0, oy + 22.0 + 14.0 - f, 14.0, f]);
        }
    } else {
        // The arrow filling from the left.
        let w = (24 * percent / 100) as f32;
        if w > 0.0 {
            game.native_sprite(batch, sheet, [176.0, 14.0, w, 17.0], [ox + 79.0, oy + 34.0, w, 17.0]);
        }
    }
}

/// `native_screen_tooltip`: the EU over the energy bar.
pub fn screen_tooltip(_game: &Game<'_>, block: &str, at: [f32; 2], lines: &mut Vec<String>) {
    if has_window(block) && at[0] >= 34.0 && at[0] < 42.0 && at[1] >= 22.0 && at[1] < 62.0 {
        let eu = SCREEN_DATA.with_borrow(|d| d.get(1).copied().unwrap_or(0));
        lines.push(format!("{eu} / {} EU", capacity(block)));
    }
}

// ---------------------------------------------------------------- by hand

fn say(game: &mut Game<'_>, text: &str) {
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, horizon_client::text::Text::legacy(&text.to_owned()));
    }
}

/// `native_right_click_block`: the machines without a window (the BatBox, the solar panel and the creative generator)
/// show their EU over the hotbar.
pub fn right_click(game: &mut Game<'_>, pos: Pos, _canceled: &mut bool) {
    let block = game.native_block(pos);
    if !is_machine(&block) || has_window(&block) {
        return;
    }
    let eu = game.native_with_mod_block_entity(pos, |e| eu_of(e)).unwrap_or(0);
    say(game, &format!("§e{eu} / {} EU", capacity(&block)));
}
