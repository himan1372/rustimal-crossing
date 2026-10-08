//! Showcase 2 generator: "Walkabout" edition.
//!
//! Builds a self-contained HTML page with four tabs, all data computed
//! from the real library code:
//!   1. Walkabout — third-person 3D town walk (WASD), wandering NPC
//!      villagers, talk-to-NPC with the real topic-selection weights,
//!      schedule states, and friendship bumps.
//!   2. Mailroom — NPC-generated mail samples (event/birthday/
//!      Christmas/goodbye/password) from npc_event_mail.rs.
//!   3. Graphics Lab — a real Emu64 interpreter trace over a small
//!      display list, steppable command by command.
//!   4. Sound Board — a scripted GameAudio session showing trigger-SE
//!      slot allocation, singleton rejection, replacement, and the
//!      deferred command ring.

#[path = "../../../rust/src/town_gen.rs"]
mod town_gen;
#[path = "../../../rust/src/npc.rs"]
mod npc;
#[path = "../../../rust/src/npc_ai.rs"]
mod npc_ai;
#[path = "../../../rust/src/talk_topics.rs"]
mod talk_topics;
#[path = "../../../rust/src/npc_event_mail.rs"]
mod npc_event_mail;
#[path = "../../../rust/src/audio.rs"]
mod audio;
#[path = "graphics_shim.rs"]
mod graphics;

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;

use town_gen::{generate, TownPlan};

// ---------- small helpers ----------

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        s.push(T[((n >> 18) & 63) as usize] as char);
        s.push(T[((n >> 12) & 63) as usize] as char);
        s.push(if i + 1 < data.len() { T[((n >> 6) & 63) as usize] as char } else { '=' });
        s.push(if i + 2 < data.len() { T[(n & 63) as usize] as char } else { '=' });
        i += 3;
    }
    s
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ---------- town JSON (same shape as showcase 1) ----------

fn plan_json(plan: &TownPlan) -> String {
    let mut json = String::with_capacity(32_000);
    let _ = write!(
        json,
        "{{\"seed\":{},\"villagerCount\":{},\"villagers\":[",
        plan.seed, plan.villager_count
    );
    for i in 0..plan.villager_count as usize {
        if i != 0 {
            json.push(',');
        }
        let _ = write!(json, "{}", plan.villagers[i]);
    }
    json.push_str("],\"homes\":[");
    for i in 0..plan.villager_count as usize {
        if i != 0 {
            json.push(',');
        }
        let _ = write!(
            json,
            "{{\"id\":{},\"acre\":{},\"unit\":{}}}",
            plan.villagers[i], plan.house_acres[i], plan.house_units[i]
        );
    }
    json.push_str("],\"acres\":[");
    for (i, acre) in plan.acres.iter().enumerate() {
        if i != 0 {
            json.push(',');
        }
        let _ = write!(
            json,
            "{{\"feature\":{},\"ground\":{},\"pattern\":{},\"elevation\":{},\"river\":{},\"cliff\":{},\"waterfall\":{},\"infra\":{},\"cells\":[",
            acre.feature,
            acre.ground_kind,
            acre.grass_pattern,
            acre.elevation,
            acre.river_edges,
            acre.cliff_edges,
            acre.waterfall_edges,
            acre.infrastructure
        );
        for (cell_index, cell) in acre.cells.iter().enumerate() {
            if cell_index != 0 {
                json.push(',');
            }
            let _ = write!(json, "{cell}");
        }
        json.push_str("]}");
    }
    json.push_str("]}");
    json
}

// ---------- walkability bitmap + NPC roster ----------

const GRID_W: usize = 80; // 5 acres * 16
const GRID_H: usize = 96; // 6 acres * 16

/// Blocked cell kinds: Tree=2, Rock=3, House=6, HouseSign=7, HouseReserved=8.
fn cell_blocked(kind: u8) -> bool {
    matches!(kind, 2 | 3 | 6 | 7 | 8)
}

/// Build a walkability bitmap (1 bit per unit). River strips are blocked
/// unless the acre has a bridge (infra bit 0).
fn walk_bitmap(plan: &TownPlan) -> Vec<u8> {
    let mut bits = vec![0u8; GRID_W * GRID_H / 8];
    let set = |bits: &mut [u8], x: usize, z: usize, walk: bool| {
        if x < GRID_W && z < GRID_H {
            let i = z * GRID_W + x;
            if walk {
                bits[i / 8] |= 1 << (i % 8);
            } else {
                bits[i / 8] &= !(1 << (i % 8));
            }
        }
    };
    for (ai, acre) in plan.acres.iter().enumerate() {
        let ox = (ai % 5) * 16;
        let oz = (ai / 5) * 16;
        for cz in 0..16usize {
            for cx in 0..16usize {
                let kind = acre.cells[cz * 16 + cx];
                set(&mut bits, ox + cx, oz + cz, !cell_blocked(kind));
            }
        }
        // River strips: from acre center toward each river edge.
        if acre.river_edges != 0 && acre.infrastructure & 1 == 0 {
            let edges = [(1u8, 8.0, 0.0), (2, 16.0, 8.0), (4, 8.0, 16.0), (8, 0.0, 8.0)];
            for (bit, ex, ez) in edges {
                if acre.river_edges & bit == 0 {
                    continue;
                }
                // Segment (8,8) -> (ex,ez) in acre-local coords.
                for cz in 0..16usize {
                    for cx in 0..16usize {
                        let px = cx as f32 + 0.5;
                        let pz = cz as f32 + 0.5;
                        // Distance from point to segment.
                        let dx = ex - 8.0;
                        let dz = ez - 8.0;
                        let len2 = dx * dx + dz * dz;
                        let t = (((px - 8.0) * dx + (pz - 8.0) * dz) / len2.max(0.0001)).clamp(0.0, 1.0);
                        let qx = 8.0 + t * dx - px;
                        let qz = 8.0 + t * dz - pz;
                        if (qx * qx + qz * qz).sqrt() < 1.9 {
                            set(&mut bits, ox + cx, oz + cz, false);
                        }
                    }
                }
            }
        }
    }
    bits
}

fn sched_name(t: u32) -> &'static str {
    match t {
        0 => "FIELD",
        1 => "IN_HOUSE",
        2 => "SLEEP",
        3 => "STAND",
        4 => "WANDER",
        5 => "WALK_WANDER",
        _ => "SPECIAL",
    }
}

/// NPC roster JSON: id, looks/personality, house pos, spawn pos,
/// schedule at 10:00 (from the real SCHEDULE_TABLES).
fn npc_json(plan: &TownPlan) -> String {
    let mut j = String::from("[");
    let hour_sec = 10 * 3600u32;
    for i in 0..plan.villager_count as usize {
        if i != 0 {
            j.push(',');
        }
        let id = plan.villagers[i];
        let looks = (id as usize) % 6;
        let pers = npc::Personality::from_looks(looks as u8)
            .map(|p| p.english_name())
            .unwrap_or("?");
        let acre = plan.house_acres[i] as usize;
        let unit = plan.house_units[i] as usize;
        let hx = ((acre % 5) * 16 + unit % 16) as f32 + 0.5;
        let hz = ((acre / 5) * 16 + unit / 16) as f32 + 0.5;
        let (_, current, _) = npc_ai::schedule_manager_sub(looks, hour_sec, 0, 0, 0);
        // Spawn: near the house door (south side), or at house if blocked.
        let _ = write!(
            j,
            "{{\"i\":{i},\"id\":{id},\"looks\":{looks},\"pers\":\"{pers}\",\"hx\":{hx:.1},\"hz\":{hz:.1},\"sched\":{current},\"sname\":\"{}\"}}",
            sched_name(current)
        );
    }
    j.push(']');
    j
}

/// Find a walkable spawn near the town center.
fn find_spawn(bits: &[u8]) -> (f32, f32) {
    let walk = |x: usize, z: usize| bits[(z * GRID_W + x) / 8] >> ((z * GRID_W + x) % 8) & 1 == 1;
    for r in 0..40usize {
        for dz in 0..=r {
            for dx in 0..=r {
                for (sx, sz) in [(40i32, 48i32)] {
                    let x = sx + dx as i32 - r as i32 / 2;
                    let z = sz + dz as i32 - r as i32 / 2;
                    if x >= 1 && z >= 1 && (x as usize) < GRID_W - 1 && (z as usize) < GRID_H - 1
                        && walk(x as usize, z as usize)
                    {
                        return (x as f32 + 0.5, z as f32 + 0.5);
                    }
                }
            }
        }
    }
    (40.5, 48.5)
}

// ---------- mailroom samples (all from the real generators) ----------

fn present_str(p: npc_event_mail::PresentRequest) -> String {
    use npc_event_mail::PresentRequest as P;
    match p {
        P::None => "none".to_string(),
        P::Catalog { kind, listtype } => {
            let k = match kind { 0 => "furniture", 1 => "clothing", _ => "?" };
            let l = match listtype { 0 => "RARE", 1 => "UNCOMMON", 2 => "COMMON", _ => "?" };
            format!("{k} [{l}]")
        }
        P::Umbrella => "umbrella".to_string(),
        P::FixedItem(_) => "fixed item".to_string(),
    }
}

fn paper_str(p: npc_event_mail::PaperSpec) -> String {
    match p {
        npc_event_mail::PaperSpec::RandomAbc => "random ABC".to_string(),
        npc_event_mail::PaperSpec::Fixed(n) => format!("fixed #{n}"),
    }
}

fn mail_json() -> String {
    use npc_event_mail::*;
    let mut rng = TestRng::new(1234);
    let mut j = String::from("[");
    let mut first = true;
    let mut push = |system: &str, title: &str, m: &GeneratedMail, delivery: &str| {
        if !first {
            j.push(',');
        }
        first = false;
        let _ = write!(
            j,
            "{{\"system\":\"{system}\",\"title\":\"{title}\",\"no\":\"0x{:X}\",\"type\":\"{:?}\",\"present\":\"{}\",\"paper\":\"{}\",\"delivery\":\"{delivery}\",\"free\":[",
            m.mail_no, m.mail_type, present_str(m.present), paper_str(m.paper)
        );
        for (i, (slot, text)) in m.free_strings.iter().enumerate() {
            if i != 0 {
                j.push(',');
            }
            let _ = write!(j, "[{slot},\"{}\"]", esc(text));
        }
        j.push_str("]}");
    };

    // Valentine: all three relationship classes.
    for (t, name) in [
        (EventMailType::BestFriend, "best friend"),
        (EventMailType::OkFriend, "good friend"),
        (EventMailType::NotFriend, "other friend"),
    ] {
        let m = make_event_mail("Philip", "Rosie", 101, 0, 1, t);
        push("Valentine", &format!("Valentine ({name})"), &m, "mailbox → post office");
    }
    // Birthday.
    let m = make_birthday_card(&mut rng, "Philip", "Bob", "cozy chair", 102, 0, 2);
    push("Birthday", "Birthday card", &m, "mailbox → post office");
    // Christmas.
    let m = make_xmas_card(0);
    push("Christmas", "Christmas card", &m, "mailbox only");
    // Goodbye.
    let pend = GoodbyePending::new(103, 4, 0b0011);
    if let Some(m) = make_goodbye_mail(&mut rng, "Philip", "Mitzi", "Rustimal", &pend, 0) {
        push("Goodbye", "Goodbye letter", &m, "mailbox → post office");
    }
    // Password: Famicom valid + invalid.
    for (valid, name) in [(true, "valid code"), (false, "invalid code")] {
        let r = make_famicom_response("Philip", "Bunnie", "ABCD", "EFGH", Some("NES console"), 104, 0, 0, valid);
        push("Password/Famicom", &format!("Famicom ({name})"), &r.mail, "post office only");
    }
    // Password: magazine win/lose/invalid.
    for (v, name) in [
        (PasswordVerdict::ValidWin, "win"),
        (PasswordVerdict::ValidLose, "lose"),
        (PasswordVerdict::Invalid, "invalid"),
    ] {
        let m = load_npc_mail_data_common2(
            magazine_mail_no(3, v), 105, 0,
            if v == PasswordVerdict::ValidWin { PresentRequest::FixedItem(0) } else { PresentRequest::None },
            PaperSpec::RandomAbc,
            vec![(0, "Philip".into()), (1, "Nibbles".into()), (6, "Nibbles".into())],
        );
        push("Password/Magazine", &format!("Magazine ({name})"), &m, "post office only");
    }
    // user_password valid/invalid.
    if let Some(no) = user_password_mail_no(2) {
        let m = load_npc_mail_data_common2(
            no, 106, 0, PresentRequest::None, PaperSpec::RandomAbc,
            vec![(6, "Tangy".into())],
        );
        push("Password/User", "User password (valid)", &m, "post office only");
    }
    let m = load_npc_mail_data_common2(
        password_ng_mail_no(2), 106, 0, PresentRequest::None, PaperSpec::RandomAbc,
        vec![(0, "Philip".into()), (6, "Tangy".into())],
    );
    push("Password/User", "User password (invalid)", &m, "post office only");
    j.push(']');
    j
}

// ---------- graphics lab trace ----------

struct TraceMem {
    verts: Vec<graphics::vertex::Vtx>,
}
impl graphics::interpreter::GfxMemory for TraceMem {
    fn vertices(&self, _a: u32, n: usize) -> Option<Vec<graphics::vertex::Vtx>> {
        Some(self.verts.iter().take(n).cloned().collect())
    }
    fn bytes(&self, _a: u32, n: usize) -> Option<Vec<u8>> {
        Some(vec![0u8; n])
    }
}

struct TraceBackend {
    pub tris: u32,
}
impl graphics::interpreter::GxBackend for TraceBackend {
    fn emit_triangle(&mut self, _t: graphics::interpreter::EmittedTri) {
        self.tris += 1;
    }
    fn emit_quad(&mut self, _v: [graphics::vertex::DecodedVtx; 4]) {}
    fn emit_texrect(&mut self, _a: u16, _b: u16, _c: u16, _d: u16) {}
    fn gx_call_display_list(&mut self, _b: &[u8]) {}
    fn cull_display_list(&mut self, _v0: u8, _vn: u8) -> bool {
        false
    }
}

fn gfx(opcode: u8, param: u8, len: u16, addr: u32) -> graphics::command::Gfx {
    graphics::command::Gfx::new(((opcode as u32) << 24) | ((param as u32) << 16) | (len as u32), addr)
}

fn gfx_trace_json() -> String {
    use graphics::command::op;
    use graphics::interpreter::Emu64;
    let mut emu = Emu64::new();
    emu.segments.set(1, 0);
    let mut lists: HashMap<u32, Vec<graphics::command::Gfx>> = HashMap::new();
    // Main list: state setup, 3 verts, 1 triangle, nested DL, end.
    // G_VTX packs n in bits 12-19 and vn=(v0+n) in bits 1-7.
    let vtx_w0 = ((op::G_VTX as u32) << 24) | (3 << 12) | ((0 + 3) << 1);
    lists.insert(
        0x1000,
        vec![
            gfx(op::G_GEOMETRYMODE, 0, 0, 0x0002_0401), // ZBUFFER|SHADE|CULL_BACK-ish
            gfx(op::G_SETCOMBINE, 0, 0, 0x1234_5678),
            gfx(op::G_SETPRIMCOLOR, 0, 0, 0xFF80_40FF),
            graphics::command::Gfx::new(vtx_w0, (1 << 24) | 0x100),
            gfx(op::G_TRI1, 0, 0, 0x00_02_04), // indices 0,2,4 -> verts 0,1,2
            gfx(op::G_DL, 0, 0, (1 << 24) | 0x2000), // PUSH
            gfx(op::G_ENDDL, 0, 0, 0),
        ],
    );
    // Nested list: env color + packed triangle + end.
    let trin_w0 = ((op::G_TRIN as u32) << 24) | (0 << 17); // 1 face
    lists.insert(
        0x2000,
        vec![
            gfx(op::G_SETENVCOLOR, 0, 0, 0x20A0_30FF),
            graphics::command::Gfx::new(trin_w0, (0 << 4) | (1 << 9) | (2 << 14)),
            gfx(op::G_ENDDL, 0, 0, 0),
        ],
    );
    let mem = TraceMem {
        verts: vec![
            graphics::vertex::Vtx { x: 0, y: 0, z: 0, ..Default::default() },
            graphics::vertex::Vtx { x: 10, y: 0, z: 0, ..Default::default() },
            graphics::vertex::Vtx { x: 0, y: 10, z: 0, ..Default::default() },
        ],
    };
    let mut be = TraceBackend { tris: 0 };
    emu.taskstart(&mem, &mut be, &lists, 0x1000);
    let mut j = format!(
        "{{\"tris\":{},\"cmds\":[",
        be.tris
    );
    for (i, t) in emu.trace.iter().enumerate() {
        if i != 0 {
            j.push(',');
        }
        let _ = write!(
            j,
            "{{\"list\":\"0x{:X}\",\"pc\":{},\"op\":\"{}\",\"dl\":{},\"dirty\":[{}],\"geo\":\"0x{:X}\",\"prim\":[{},{},{},{}]}}",
            t.list_addr,
            t.pc,
            t.opcode_name,
            t.dl_level,
            t.dirty.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(","),
            t.geometry_mode,
            t.prim_rgba[0], t.prim_rgba[1], t.prim_rgba[2], t.prim_rgba[3]
        );
    }
    j.push_str("]}");
    j
}

// ---------- sound board trace ----------

fn audio_trace_json() -> String {
    use audio::*;
    let mut g = GameAudio::new();
    let mut steps: Vec<String> = Vec::new();
    let mut snap = |label: &str, g: &GameAudio, ring: &CommandRing| {
        let mut s = format!("{{\"label\":\"{label}\",\"slots\":[");
        for (i, sl) in g.trg.slots.iter().enumerate() {
            if i != 0 {
                s.push(',');
            }
            let _ = write!(s, "{{\"id\":\"0x{:X}\",\"frames\":{},\"prio\":{}}}", sl.id, sl.frames, sl.priority);
        }
        let _ = write!(s, "],\"ring\":{}}}", ring.pending());
        s
    };
    let mut ring = std::mem::take(&mut g.ring);
    // 1: normal SE -> slot 0.
    let r1 = g.trg.start(&mut ring, 0x0001, 1.0, 1.0, 1.0, 0x40, 0, 100.0);
    steps.push(snap(&format!("start 0x0001 -> {r1:?}"), &g, &ring));
    // 2: singleton SE twice -> second rejected.
    let r2 = g.trg.start(&mut ring, 0x8005, 1.0, 1.0, 1.0, 0x40, 0, 100.0);
    let r3 = g.trg.start(&mut ring, 0x8005, 1.0, 1.0, 1.0, 0x40, 0, 100.0);
    steps.push(snap(&format!("singleton 0x8005 -> {r2:?}, again -> {r3:?}"), &g, &ring));
    // 3: fill remaining slots.
    for k in 2..6u16 {
        let r = g.trg.start(&mut ring, 0x0010 + k, 1.0, 1.0, 1.0, 0x40, 0, 100.0);
        steps.push(snap(&format!("fill slot -> {r:?}"), &g, &ring));
    }
    // 4: one more -> oldest-frames replacement gated by TRGPRIO.
    let r = g.trg.start(&mut ring, 0x0069, 1.0, 1.0, 1.0, 0x40, 0, 100.0);
    steps.push(snap(&format!("overflow 0x0069 -> {r:?}"), &g, &ring));
    // 5: MONO SE bypasses slots entirely.
    let r = g.trg.start(&mut ring, 0x1002, 1.0, 1.0, 1.0, 0x40, 0, 100.0);
    steps.push(snap(&format!("MONO 0x1002 -> {r:?}"), &g, &ring));
    // 6: voice spec change -> sequence 243 (Animalese).
    let seq = g.voice.spec_change(&mut ring, 2);
    steps.push(snap(&format!("voice: spec 2 -> seq {seq} (Animalese)"), &g, &ring));
    g.ring = ring;
    format!("[{}]", steps.join(","))
}

// ---------- main ----------

fn run() -> Result<(), String> {
    let seed = 305419896u32;
    let villager_count = 6usize;
    let resident_ids: Vec<u16> = (0..villager_count).map(|i| 1000 + i as u16).collect();
    let (plan, used_seed) = (0..64u32)
        .find_map(|retry| {
            let candidate_seed = seed.wrapping_add(retry);
            generate(candidate_seed, &resident_ids, villager_count)
                .map(|plan| (plan, candidate_seed))
        })
        .ok_or("the Rust town planner could not create a valid town within 64 seed attempts")?;
    println!("Town seed: {used_seed} | residents: {}", plan.villager_count);

    let town = plan_json(&plan);
    let bits = walk_bitmap(&plan);
    let spawn = find_spawn(&bits);
    let walk = format!(
        "{{\"walk\":\"{}\",\"npcs\":{},\"spawn\":{{\"x\":{:.1},\"z\":{:.1}}}}}",
        b64(&bits),
        npc_json(&plan),
        spawn.0,
        spawn.1
    );
    println!("Walk bitmap: {} bytes, spawn {:?}", bits.len(), spawn);

    let mail = mail_json();
    println!("Mail samples: {} entries", mail.matches("\"system\"").count());
    let gfx = gfx_trace_json();
    println!("GFX trace: {} commands", gfx.matches("\"op\"").count());
    let snd = audio_trace_json();
    println!("Sound steps: {}", snd.matches("\"label\"").count());

    let mut page = include_str!("showcase2_template.html").to_string();
    page = page
        .replace("__TOWN_JSON__", &town)
        .replace("__WALK_JSON__", &walk)
        .replace("__MAIL_JSON__", &mail)
        .replace("__GFX_JSON__", &gfx)
        .replace("__SND_JSON__", &snd);

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no output dir")?;
    let path = dir.join("showcase2.html");
    fs::write(&path, page).map_err(|e| format!("write failed: {e}"))?;
    println!("Showcase 2 written to: {}", path.display());
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("Error: {message}");
        std::process::exit(2);
    }
}
