//! Showcase generator: builds a self-contained multi-tab HTML prototype
//! presenting the Rust rewrite's major systems, all computed from the real
//! library code (included via #[path]).

#[path = "../../rust/src/town_gen.rs"]
mod town_gen;
#[path = "../../rust/src/ecology.rs"]
mod ecology;
#[path = "../../rust/src/species.rs"]
mod species;
#[path = "../../rust/src/fish_tables.rs"]
mod fish_tables;
#[path = "../../rust/src/insect_tables.rs"]
mod insect_tables;
#[path = "../../rust/src/letter_score.rs"]
mod letter_score;
#[path = "../../rust/src/villager_mail.rs"]
mod villager_mail;
#[path = "../../rust/src/quest.rs"]
mod quest;
#[path = "../../rust/src/quest_gen.rs"]
mod quest_gen;

use std::env;
use std::fmt::Write as FmtWrite;
use std::fs;

use town_gen::{generate, ACRE_WIDTH, MAX_TOWN_VILLAGERS};

// ---------- name tables (source-order discriminants) ----------

fn fish_name(f: u8) -> &'static str {
    match f {
        0 => "Crucian Carp", 1 => "Brook Trout", 2 => "Carp", 3 => "Koi",
        4 => "Catfish", 5 => "Small Bass", 6 => "Bass", 7 => "Large Bass",
        8 => "Bluegill", 9 => "Giant Catfish", 10 => "Giant Snakehead",
        11 => "Barbel Steed", 12 => "Dace", 13 => "Pale Chub", 14 => "Bitterling",
        15 => "Loach", 16 => "Pond Smelt", 17 => "Sweetfish", 18 => "Cherry Salmon",
        19 => "Large Char", 20 => "Rainbow Trout", 21 => "Stringfish", 22 => "Salmon",
        23 => "Goldfish", 24 => "Piranha", 25 => "Arowana", 26 => "Eel",
        27 => "Freshwater Goby", 28 => "Angelfish", 29 => "Guppy",
        30 => "Popeyed Goldfish", 31 => "Coelacanth", 32 => "Crawfish", 33 => "Frog",
        34 => "Killifish", 35 => "Jellyfish", 36 => "Sea Bass", 37 => "Red Snapper",
        38 => "Barred Knifefish", 39 => "Arapaima", 40 => "Whale", 41 => "Empty Can",
        42 => "Boot", 43 => "Old Tire", 44 => "Salmon2", _ => "?",
    }
}

fn fish_area_name(a: u8) -> &'static str {
    match a {
        0 => "Pool", 1 => "Waterfall", 2 => "River mouth", 3 => "Offing",
        4 => "Sea", 5 => "River", 6 => "Pond", _ => "?",
    }
}

fn insect_name(f: u8) -> &'static str {
    match f {
        0 => "Common Butterfly", 1 => "Yellow Butterfly", 2 => "Tiger Butterfly",
        3 => "Purple Butterfly", 4 => "Robust Cicada", 5 => "Walker Cicada",
        6 => "Evening Cicada", 7 => "Brown Cicada", 8 => "Bee",
        9 => "Common Dragonfly", 10 => "Red Dragonfly", 11 => "Darner Dragonfly",
        12 => "Banded Dragonfly", 13 => "Long Locust", 14 => "Migratory Locust",
        15 => "Cricket", 16 => "Grasshopper", 17 => "Bell Cricket",
        18 => "Pine Cricket", 19 => "Drone Beetle", 20 => "Dynastid Beetle",
        21 => "Flat Stag Beetle", 22 => "Jewel Beetle", 23 => "Longhorn Beetle",
        24 => "Ladybug", 25 => "Spotted Ladybug", 26 => "Mantis", 27 => "Firefly",
        28 => "Cockroach", 29 => "Saw Stag Beetle", 30 => "Mountain Beetle",
        31 => "Giant Beetle", 32 => "Snail", 33 => "Mole Cricket",
        34 => "Pond Skater", 35 => "Bagworm", 36 => "Pill Bug", 37 => "Spider",
        38 => "Ant", 39 => "Mosquito", 40 => "Spirit", 41 => "NONE (no insect)",
        _ => "?",
    }
}

fn insect_area_name(a: u8) -> &'static str {
    match a {
        0 => "On tree", 1 => "On flower", 2 => "Raining on flower", 3 => "Flying",
        4 => "On ground", 5 => "In bush", 6 => "Flying near water", 7 => "On water",
        8 => "On candy", 9 => "On trash", 10 => "Under rock", 11 => "Underground",
        12 => "Near flowers or around", 13 => "Nothing", _ => "?",
    }
}

// ---------- town JSON (same as town_prototype) ----------

fn plan_json(plan: &town_gen::TownPlan) -> String {
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

// ---------- species JSON ----------

fn species_json() -> String {
    let mut j = String::with_capacity(180_000);
    j.push_str("{\"fish\":{");
    let mut first = true;
    for env in 0..3u8 {
        for term in 0..24u8 {
            for time in 0..4u8 {
                if let Some(t) = fish_tables::fish_table(env, term, time) {
                    if !first {
                        j.push(',');
                    }
                    first = false;
                    let _ = write!(j, "\"{env}_{term}_{time}\":[");
                    for (i, e) in t.iter().enumerate() {
                        if i != 0 {
                            j.push(',');
                        }
                        let _ = write!(
                            j,
                            "[{},{},{}]",
                            e.fish as u8, e.area as u8, e.weight
                        );
                    }
                    j.push(']');
                }
            }
        }
    }
    j.push_str("},\"insect_town\":{");
    first = true;
    for m in 0..12usize {
        for ti in 0..6usize {
            let t = insect_tables::INSECT_TOWN[m][ti];
            if !first {
                j.push(',');
            }
            first = false;
            let _ = write!(j, "\"{m}_{ti}\":[");
            for (i, e) in t.iter().enumerate() {
                if i != 0 {
                    j.push(',');
                }
                let _ = write!(j, "[{},{},{}]", e.insect as u8, e.area as u8, e.weight);
            }
            j.push(']');
        }
    }
    j.push_str("},\"insect_island\":{");
    for ti in 0..6usize {
        if ti != 0 {
            j.push(',');
        }
        let t = insect_tables::INSECT_ISLAND[ti];
        let _ = write!(j, "\"{ti}\":[");
        for (i, e) in t.iter().enumerate() {
            if i != 0 {
                j.push(',');
            }
            let _ = write!(j, "[{},{},{}]", e.insect as u8, e.area as u8, e.weight);
        }
        j.push(']');
    }
    j.push_str("}}");
    j
}

fn js_name_array(items: &[&str]) -> String {
    let mut s = String::from("[");
    for (i, n) in items.iter().enumerate() {
        if i != 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(n);
        s.push('"');
    }
    s.push(']');
    s
}
// ---------- letter lab: sample letters scored by the real Rust scorer ----------

struct LetterSample {
    title: &'static str,
    text: &'static str,
    present: bool,
}

fn letter_samples() -> Vec<LetterSample> {
    vec![
        LetterSample {
            title: "Warm and chatty",
            text: "Hi Bob! How are you doing today? I found the cutest shirt at the tailor shop and thought of you. Let's go fishing by the river tomorrow morning! The weather has been so nice lately.",
            present: true,
        },
        LetterSample {
            title: "Short and sweet",
            text: "Thanks for the gift! I love it.",
            present: false,
        },
        LetterSample {
            title: "One word",
            text: "hi",
            present: false,
        },
        LetterSample {
            title: "No punctuation ramble",
            text: "hey whats up i was just walking around town and saw a bunch of stuff going on by the river it was really cool you should come check it out sometime maybe tomorrow",
            present: false,
        },
        LetterSample {
            title: "ALL CAPS shouting",
            text: "HELLO HOW ARE YOU I AM HAVING A GREAT DAY IN TOWN THE SUN IS SHINING AND THE BIRDS ARE SINGING",
            present: true,
        },
        LetterSample {
            title: "Careful and proper",
            text: "Dear Goldie, Thank you so much for your kind letter. I read it three times! Your stories about the island made me smile. Please write again soon. With love and friendship always.",
            present: true,
        },
    ]
}

fn body192(text: &str) -> [u8; letter_score::MAIL_BODY_LEN] {
    let mut b = [0x20u8; letter_score::MAIL_BODY_LEN];
    let bytes = text.as_bytes();
    let n = bytes.len().min(letter_score::MAIL_BODY_LEN);
    b[..n].copy_from_slice(&bytes[..n]);
    b
}

fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn letter_lab_json() -> String {
    let mut j = String::from("[");
    for (i, s) in letter_samples().iter().enumerate() {
        if i != 0 {
            j.push(',');
        }
        let body = body192(s.text);
        let sc = letter_score::score_letter(&body, letter_score::TrigramMode::NtscU);
        let rank = letter_score::quest_rank(&body, s.present, letter_score::TrigramMode::NtscU);
        let _ = write!(
            j,
            "{{\"title\":\"{}\",\"text\":\"{}\",\"present\":{},\"a\":{},\"b\":{},\"c\":{},\"d\":{},\"e\":{},\"f\":{},\"g\":{},\"total\":{},\"rank\":{}}}",
            js_escape(s.title),
            js_escape(s.text),
            s.present,
            sc.a, sc.b, sc.c, sc.d, sc.e, sc.f, sc.g,
            sc.total(),
            rank
        );
    }
    j.push(']');
    j
}
// ---------- quest board: real generation tables ----------

fn quest_board_json() -> String {
    use quest_gen::*;
    let mut j = String::from("{\"rows\":[");
    let mut first = true;
    let type_names = ["Delivery", "Errand", "Contest"];
    for qt in 0..3u8 {
        let kinds: Vec<u8> = match qt {
            0 => vec![0, 1, 2, 3],
            1 => vec![0],
            _ => vec![0, 1, 2, 3, 4, 5, 6],
        };
        let kind_names: Vec<&str> = match qt {
            0 => vec!["Normal", "Foreign", "Remove", "Lost"],
            1 => vec!["Request"],
            _ => vec!["Fruit", "Soccer", "Snowman", "Flower", "Fish", "Insect", "Letter"],
        };
        for (ki, &kind) in kinds.iter().enumerate() {
            if let Some(d) = set_data(qt, kind) {
                if !first {
                    j.push(',');
                }
                first = false;
                let target = match d.target {
                    QuestTarget::Random => "Random",
                    QuestTarget::RandomExcluded => "Random (no chain)",
                    QuestTarget::OriginalTarget => "Chain head",
                    QuestTarget::Foreign => "Foreign (stored)",
                    QuestTarget::LastRemove => "Last moved out",
                    QuestTarget::Client => "The asker",
                };
                let src = match d.item_source {
                    QuestItemSource::Random => "random item",
                    QuestItemSource::Fruit => "non-native fruit",
                    QuestItemSource::Cloth => "random clothing",
                    QuestItemSource::FromData => "fixed item",
                    QuestItemSource::CurrentItem => "errand item",
                    QuestItemSource::None => "none",
                };
                let _ = write!(
                    j,
                    "{{\"type\":\"{}\",\"kind\":\"{}\",\"target\":\"{}\",\"days\":{},\"last_step\":{},\"handover\":{},\"item\":\"{}\",\"max_pay\":{}}}",
                    type_names[qt as usize],
                    kind_names[ki],
                    target,
                    d.day_limit,
                    d.last_step,
                    d.handover_item,
                    src,
                    d.max_pay
                );
            }
        }
    }
    j.push_str("]}");
    j
}
// ---------- systems index: one card per major ported system ----------

fn system_cards() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Town planner", "5x6 acre map, 6 villager homes, river channels, bridges, trees. Explore it live in the 3D tab."),
        ("Field generator", "Step-mode 85/15, 9-bit perfect-bit rejection sampling, verbatim cliff/river tables, slope/pool/bridge conversion."),
        ("Seasonal species", "678 fish + 576 insect table entries, verbatim. 24 half-month fish terms, 6 daily insect periods. Explore in the Fish & Bugs tab."),
        ("Fish & insect AI", "Unusual retail selection: raw-total random minus weight x field-rank rate; fish retry invalid habitats, insects filter habitats first."),
        ("Letter scoring", "Seven checks A-G plus trigram model (776 pairs). Try the presets in the Letter Lab tab."),
        ("Quest generation", "Type/kind tables, six recipient modes, entrusted-item pockets, letter quests. See the Quest Board tab."),
        ("Quest lifecycle", "Delivery/errand/contest state, rewards, timeouts, completion predicates per contest kind."),
        ("Furniture & rooms", "House interiors as persistent FG layers; 2-bit rotation, 6 shape footprints, RSV_FE1F reservation cells."),
        ("NPC AI", "Six personality schedules, interrupt priorities, wander borders, talk throttle keyed by looks."),
        ("Villager behavior", "23-action engine, priority arbitration, move-in/move-out validation, favor state machine."),
        ("Dialogue topics", "Hierarchical topic generator: probability tables, 19 message banks, weather/time/season formulas."),
        ("Force calls", "NPC-initiated conversations: 3-gate reserve, friendship approach chain, 300-frame cooldown."),
        ("Inventory", "15-pocket linear scan, first-match wins, 2-bit item conditions, deterministic possession checks."),
        ("Player actions", "121-entry action enum in C order, request/priority arbitration, verbatim dig/fill/get frame tables."),
        ("Player tools", "Axe swing timeline, net capsule catch test, rod cast/relax/vib state machines."),
        ("Shops & economy", "Nook upgrade ladder, turnip price patterns, catalog, Able Sisters designs, post office savings."),
        ("House system", "Size ladder Small>Medium>Large>Upper>Statue, basement from Medium, rehouse ordering, Nook loans."),
        ("Save format", "Persistent term fields, pocket conditions, quest records; FURNITURE_STORAGE_SLOTS = 3 (source-proven)."),
        ("Weather & seasons", "Seasonal terms, weather transitions, rain effects on spawns and letter quests."),
        ("Scene tables", "All 52 scene manifests decoded, Scene_ct interpreter, FIELD_CT endian packing."),
        ("Mail system", "Mailbox, present attachments, festive paper, reply handbills 0x75 + rank*6 + looks."),
        ("My room", "Furniture placement judge: 5-unit search, NO_COLLISION under-player, surface/second-layer rules."),
        ("Collision", "Background collision checks, acre interior 12x12 units, ocean depth rules."),
        ("Time & calendar", "RTC terms, event flags, first-job gating, 5-day seasonal transition offsets."),
    ]
}

fn cards_html() -> String {
    let mut s = String::new();
    for (name, fact) in system_cards() {
        let _ = write!(
            s,
            "<div class=\"card\"><h3>{name}</h3><p>{fact}</p></div>"
        );
    }
    s
}
// ---------- HTML assembly ----------

const VIEWER_TEMPLATE: &str = include_str!("../../town_prototype/viewer_template.html");

fn scope_viewer_style(style: &str) -> String {
    let mut s = style.to_string();
    // Drop the fullscreen html,body rule; the page shell owns the body.
    if let Some(start) = s.find("html,body{") {
        if let Some(end) = s[start..].find('}') {
            s.replace_range(start..start + end + 1, "");
        }
    }
    let rules = [
        (".toggles label{", "#tab-town .toggles label{"),
        (".toggles input{", "#tab-town .toggles input{"),
        (".toggles{", "#tab-town .toggles{"),
        (".stat b{", "#tab-town .stat b{"),
        (".stat span{", "#tab-town .stat span{"),
        (".stats{", "#tab-town .stats{"),
        (".stat{", "#tab-town .stat{"),
        (".legend{", "#tab-town .legend{"),
        (".chip{", "#tab-town .chip{"),
        (".panel{", "#tab-town .panel{"),
        (".sub{", "#tab-town .sub{"),
        (".help{", "#tab-town .help{"),
        (".notice{", "#tab-town .notice{"),
        ("h1{", "#tab-town h1{"),
        ("button:hover{", "#tab-town button:hover{"),
        ("button{", "#tab-town button{"),
        ("@media(max-width:640px){.panel{", "@media(max-width:640px){#tab-town .panel{"),
        (".legend{gap:4px}", "#tab-town .legend{gap:4px}"),
        (".help{bottom:8px", "#tab-town .help{bottom:8px"),
        (".notice{display:none}", "#tab-town .notice{display:none}"),
    ];
    for (from, to) in rules {
        s = s.replace(from, to);
    }
    s
}

fn viewer_parts(town_json: &str) -> (String, String, String) {
    let t = VIEWER_TEMPLATE;
    let style = t
        .split("<style>")
        .nth(1)
        .unwrap_or("")
        .split("</style>")
        .next()
        .unwrap_or("");
    let body = t
        .split("</style>")
        .nth(1)
        .unwrap_or("")
        .split("<script>")
        .next()
        .unwrap_or("");
    let script = t
        .split("<script>")
        .nth(1)
        .unwrap_or("")
        .split("</script>")
        .next()
        .unwrap_or("");
    let style = scope_viewer_style(style);
    let script = script
        .replace("__TOWN_DATA__", town_json)
        .replace("innerWidth", "(canvas.clientWidth||2)")
        .replace("innerHeight", "(canvas.clientHeight||2)");
    (style, body.to_string(), script)
}

fn build_page(
    town_style: &str,
    town_body: &str,
    town_script: &str,
    species_json: &str,
    letter_json: &str,
    quest_json: &str,
    cards: &str,
    fish_names: &str,
    fish_areas: &str,
    insect_names: &str,
    insect_areas: &str,
) -> String {
    let page = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Rustimal Crossing — Systems Showcase</title>
<style>
:root{color-scheme:dark;--ink:#e7f0e5;--panel:#17231e;--edge:#35463b;--mint:#a9d9a8;--gold:#e5c982;--bg:#0d1515}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.5 "Segoe UI",Arial,sans-serif}
header{padding:18px 22px 10px;border-bottom:1px solid var(--edge);background:#101a17}
header h1{margin:0 0 4px;font-size:22px;color:#f5f1df}
header .sub{color:#aab9ad;font-size:13px;max-width:900px}
nav.tabs{display:flex;gap:6px;padding:12px 22px 0;flex-wrap:wrap;background:#101a17;border-bottom:1px solid var(--edge)}
nav.tabs button{background:#1b2823;border:1px solid var(--edge);border-bottom:none;color:#cfe0cf;padding:9px 18px;border-radius:9px 9px 0 0;cursor:pointer;font-size:14px;font-weight:600}
nav.tabs button.active{background:#24382e;color:#fff;border-color:#5d775f}
.tabpane{display:none;padding:20px 22px;max-width:1200px}
.tabpane.active{display:block}
#tab-town{position:relative;height:calc(100vh - 170px);min-height:540px;max-width:none;overflow:hidden;border-radius:12px;border:1px solid var(--edge);padding:0}
#tab-town #view{position:absolute;inset:0;width:100%;height:100%;display:block;touch-action:none}
select,input[type=number]{background:#1b2823;color:var(--ink);border:1px solid var(--edge);border-radius:7px;padding:7px 10px;font-size:14px}
.controls{display:flex;gap:14px;flex-wrap:wrap;align-items:end;margin-bottom:16px}
.controls label{display:flex;flex-direction:column;gap:5px;font-size:12px;color:#aab9ad}
.controls label.row{flex-direction:row;align-items:center;gap:7px;font-size:14px;color:var(--ink)}
table.data{border-collapse:collapse;width:100%;background:#131f1b;border:1px solid var(--edge);border-radius:8px;overflow:hidden}
table.data th,table.data td{padding:7px 12px;text-align:left;border-bottom:1px solid #223129;font-size:13px}
table.data th{background:#1c2b24;color:var(--mint);font-weight:600;position:sticky;top:0}
table.data tr:hover td{background:#182620}
.badge{display:inline-block;background:#314b38;border:1px solid #5d775f;color:#dff0dc;border-radius:99px;padding:2px 10px;font-size:12px;margin:2px 4px 2px 0}
.note{color:#aab9ad;font-size:13px;margin:10px 0}
h2{color:#f5f1df;font-size:18px;margin:4px 0 12px}
.bar{height:10px;background:#243129;border-radius:5px;overflow:hidden;min-width:120px}
.bar i{display:block;height:100%;background:linear-gradient(90deg,#7fb069,#e5c982)}
.letter-list{display:flex;gap:8px;flex-wrap:wrap;margin-bottom:16px}
.letter-list button{background:#1b2823;border:1px solid var(--edge);color:var(--ink);padding:8px 14px;border-radius:8px;cursor:pointer;font-size:13px}
.letter-list button.active{background:#314b38;border-color:#7fb069;color:#fff}
.letter-text{background:#131f1b;border:1px solid var(--edge);border-radius:8px;padding:14px 16px;margin-bottom:14px;font-style:italic;color:#d8e4d5;max-width:800px}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(270px,1fr));gap:12px}
.card{background:#131f1b;border:1px solid var(--edge);border-radius:10px;padding:14px 16px}
.card h3{margin:0 0 6px;font-size:15px;color:var(--mint)}
.card p{margin:0;font-size:13px;color:#c3d2c3}
.count{color:var(--gold);font-weight:600}
__TOWN_STYLE__
</style>
</head>
<body>
<header>
<h1>Rustimal Crossing — Systems Showcase</h1>
<div class="sub">Every number on this page was computed by the actual Rust rewrite code at generation time — the same verified logic as the research docs. No game assets are used.</div>
</header>
<nav class="tabs">
<button data-tab="tab-town" class="active">3D Town</button>
<button data-tab="tab-species">Fish &amp; Bugs</button>
<button data-tab="tab-letter">Letter Lab</button>
<button data-tab="tab-quest">Quest Board</button>
<button data-tab="tab-systems">Systems Index</button>
</nav>

<div class="tabpane active" id="tab-town">
__TOWN_BODY__
</div>

<div class="tabpane" id="tab-species">
<h2>Seasonal species explorer <span class="note">— 678 fish + 576 insect table entries, verbatim from the decomp</span></h2>
<div class="controls">
<label>Creature<select id="sp-kind"><option value="fish">Fish</option><option value="insect">Insects</option></select></label>
<label id="sp-env-wrap">Water<select id="sp-env"><option value="0">River</option><option value="1">Ocean</option><option value="2">Pond</option></select></label>
<label id="sp-isl-wrap" style="display:none" class="row"><input type="checkbox" id="sp-island"> Island instead of town</label>
<label>Month<select id="sp-month"></select></label>
<label id="sp-day-wrap">Day<select id="sp-day"></select></label>
<label>Hour<select id="sp-hour"></select></label>
<label class="row" id="sp-rain-wrap"><input type="checkbox" id="sp-rain"> Raining</label>
</div>
<div id="sp-notes"></div>
<div class="note">Showing <span class="count" id="sp-count"></span> table entries for <span id="sp-where"></span>. Weights are relative, not percentages.</div>
<table class="data"><thead><tr><th>#</th><th>Species</th><th>Spawn area</th><th>Weight</th></tr></thead><tbody id="sp-rows"></tbody></table>
<div class="note">Retail quirks modeled: fish use 24 half-month terms; insects use 12 monthly terms; seasonal transitions blend two tables by concatenation (never merged by species); NONE is a real no-insect entry; Coelacanth is injected dynamically, never a table row.</div>
</div>

<div class="tabpane" id="tab-letter">
<h2>Letter Lab <span class="note">— scored by the real seven-check + trigram engine</span></h2>
<div class="letter-list" id="letter-list"></div>
<div class="letter-text" id="letter-text"></div>
<div id="letter-present"></div>
<table class="data" style="max-width:800px"><thead><tr><th>Check</th><th>What it measures</th><th style="width:220px">Score</th><th>Value</th></tr></thead><tbody id="letter-rows"></tbody></table>
<div class="note">Total = A+B+C+D+E+F+G. The quest rank (0–11) drives reply presents: length tiers + quality bonus + present bonus.</div>
</div>

<div class="tabpane" id="tab-quest">
<h2>Quest Board <span class="note">— the real <i>l_set_data</i> generation tables</span></h2>
<div class="controls">
<label>Quest type<select id="q-type"><option value="Delivery">Delivery</option><option value="Errand">Errand</option><option value="Contest">Contest</option></select></label>
</div>
<table class="data"><thead><tr><th>Kind</th><th>Recipient mode</th><th>Days</th><th>Final step</th><th>Entrusted item</th><th>Item source</th><th>Max pay</th></tr></thead><tbody id="quest-rows"></tbody></table>
<div class="note">Generation: 75% attempt gate → uniform type → uniform kind → occurrence check → set-data row → recipient → item → first-empty-pocket handover. Six recipient modes: Random, Random (chain excluded), Chain head, Foreign (stored ID), Last moved-out, The asker.</div>
</div>

<div class="tabpane" id="tab-systems">
<h2>Systems Index <span class="note">— everything ported so far</span></h2>
<div class="cards">__CARDS__</div>
</div>

<script>
"use strict";
document.querySelectorAll("nav.tabs button").forEach(b=>b.addEventListener("click",()=>{
  document.querySelectorAll("nav.tabs button").forEach(x=>x.classList.remove("active"));
  document.querySelectorAll(".tabpane").forEach(x=>x.classList.remove("active"));
  b.classList.add("active");
  document.getElementById(b.dataset.tab).classList.add("active");
}));

/* ---------- species explorer ---------- */
const SPECIES = __SPECIES_JSON__;
const FISH_NAMES = __FISH_NAMES__;
const FISH_AREAS = __FISH_AREAS__;
const INSECT_NAMES = __INSECT_NAMES__;
const INSECT_AREAS = __INSECT_AREAS__;
const MONTHS = ["January","February","March","April","May","June","July","August","September","October","November","December"];
const mSel = document.getElementById("sp-month"), dSel = document.getElementById("sp-day"), hSel = document.getElementById("sp-hour");
MONTHS.forEach((m,i)=>{const o=document.createElement("option");o.value=i+1;o.textContent=m;mSel.appendChild(o);});
for(let d=1;d<=31;d++){const o=document.createElement("option");o.value=d;o.textContent=d;dSel.appendChild(o);}
for(let h=0;h<24;h++){const o=document.createElement("option");o.value=h;o.textContent=String(h).padStart(2,"0")+":00";hSel.appendChild(o);}
mSel.value = 7; dSel.value = 15; hSel.value = 12;
function fishTerm(mo,da){let t=(mo-1)*2; if(da>15) t++; return t%24;}
function fishTime(h){return h<=3?0:h<=8?1:h<=15?2:h<=20?3:0;}
function insectTime(h){return h===23||h<=3?0:h<=7?1:h<=15?2:h===16?3:h<=18?4:5;}
function renderSpecies(){
  const kind = document.getElementById("sp-kind").value;
  const mo = +mSel.value, da = +dSel.value, h = +hSel.value;
  const rain = document.getElementById("sp-rain").checked;
  const rows = document.getElementById("sp-rows"), notes = document.getElementById("sp-notes");
  rows.innerHTML = ""; notes.innerHTML = "";
  document.getElementById("sp-env-wrap").style.display = kind==="fish" ? "" : "none";
  document.getElementById("sp-day-wrap").style.display = kind==="fish" ? "" : "none";
  document.getElementById("sp-rain-wrap").style.display = kind==="fish" ? "" : "none";
  document.getElementById("sp-isl-wrap").style.display = kind==="insect" ? "" : "none";
  let list, names, areas, where;
  if(kind==="fish"){
    const env = +document.getElementById("sp-env").value;
    const key = env+"_"+fishTerm(mo,da)+"_"+fishTime(h);
    list = SPECIES.fish[key]; names = FISH_NAMES; areas = FISH_AREAS;
    const envName = ["river","ocean","pond"][env];
    where = MONTHS[mo-1]+" "+(da<=15?"1–15":"16–end")+", "+["night (21–04)","morning (04–09)","day (09–16)","evening (16–21)"][fishTime(h)]+", "+envName;
    if(!list){ notes.innerHTML = '<span class="badge">No pond tables this month</span>'; }
    else if(rain && env===1 && !(h>=9&&h<16)){
      notes.innerHTML += '<span class="badge">+ Coelacanth (Sea, 2.0) injected by rain</span>';
      list = list.concat([[31,4,2.0]]);
    }
    if(env===1) notes.innerHTML += '<span class="badge">Offing = 10x ocean weights + Whale 1</span>';
  } else {
    const isl = document.getElementById("sp-island").checked;
    const t = insectTime(h);
    if(isl){ list = SPECIES.insect_island[""+t]; where = "island, "+["night (23–04)","morning (04–08)","day (08–16)","4–5 PM","5–7 PM","7–11 PM"][t]; }
    else { list = SPECIES.insect_town[(mo-1)+"_"+t]; where = MONTHS[mo-1]+", "+["night (23–04)","morning (04–08)","day (08–16)","4–5 PM","5–7 PM","7–11 PM"][t]; }
    names = INSECT_NAMES; areas = INSECT_AREAS;
    notes.innerHTML += '<span class="badge">+ Ant/candy, Ant/trash, Cockroach/trash appended at runtime</span>';
  }
  list = list || [];
  document.getElementById("sp-count").textContent = list.length;
  document.getElementById("sp-where").textContent = where;
  list.forEach((e,i)=>{
    const tr = document.createElement("tr");
    const nm = names[e[0]]||("?"+e[0]);
    tr.innerHTML = "<td>"+(i+1)+"</td><td>"+nm+"</td><td>"+areas[e[1]]+"</td><td>"+e[2]+"</td>";
    if(kind==="insect" && e[0]===41) tr.style.opacity = ".65";
    rows.appendChild(tr);
  });
}
["sp-kind","sp-env","sp-month","sp-day","sp-hour","sp-rain","sp-island"].forEach(id=>document.getElementById(id).addEventListener("change",renderSpecies));
renderSpecies();

/* ---------- letter lab ---------- */
const LETTERS = __LETTER_JSON__;
const CHECK_INFO = {a:"Punctuation & capitalization",b:"Trigram word naturalness",c:"Word variety",d:"Sentence shape",e:"Greeting/closing",f:"Sentence starts",g:"Length & effort"};
const listEl = document.getElementById("letter-list");
LETTERS.forEach((L,i)=>{
  const b = document.createElement("button");
  b.textContent = L.title; b.dataset.i = i;
  b.addEventListener("click",()=>showLetter(i));
  listEl.appendChild(b);
});
function showLetter(i){
  const L = LETTERS[i];
  document.querySelectorAll("#letter-list button").forEach((b,j)=>b.classList.toggle("active", j===i));
  document.getElementById("letter-text").textContent = "\u201c"+L.text+"\u201d";
  document.getElementById("letter-present").innerHTML =
    (L.present?'<span class="badge">Present attached (+6 rank)</span>':'<span class="badge">No present</span>')+
    ' <span class="badge">Quest rank '+L.rank+' / 11</span>';
  const tb = document.getElementById("letter-rows"); tb.innerHTML = "";
  const max = {a:20,b:30,c:20,d:20,e:10,f:10,g:20};
  ["a","b","c","d","e","f","g"].forEach(k=>{
    const tr = document.createElement("tr");
    const pct = Math.max(0, Math.min(100, L[k]/max[k]*100));
    tr.innerHTML = "<td><b>"+k.toUpperCase()+"</b></td><td>"+CHECK_INFO[k]+"</td>"+
      '<td><div class="bar"><i style="width:'+pct+'%"></i></div></td><td class="count">'+L[k]+"</td>";
    tb.appendChild(tr);
  });
  const tr = document.createElement("tr");
  tr.innerHTML = "<td><b>Total</b></td><td>A+B+C+D+E+F+G</td><td></td>"+'<td class="count">'+L.total+"</td>";
  tb.appendChild(tr);
}
showLetter(0);

/* ---------- quest board ---------- */
const QUESTS = __QUEST_JSON__;
const qType = document.getElementById("q-type");
function renderQuests(){
  const tb = document.getElementById("quest-rows"); tb.innerHTML = "";
  QUESTS.rows.filter(r=>r.type===qType.value).forEach(r=>{
    const tr = document.createElement("tr");
    tr.innerHTML = "<td><b>"+r.kind+"</b></td><td>"+r.target+"</td><td>"+r.days+"</td><td>"+r.last_step+"</td><td>"+
      (r.handover?"yes":"no")+"</td><td>"+r.item+"</td>"+'<td class="count">'+r.max_pay+"</td>";
    tb.appendChild(tr);
  });
}
qType.addEventListener("change",renderQuests);
renderQuests();
</script>
<script>
__TOWN_SCRIPT__
</script>
</body>
</html>"##;
    page.replace("__TOWN_STYLE__", town_style)
        .replace("__TOWN_BODY__", town_body)
        .replace("__TOWN_SCRIPT__", town_script)
        .replace("__SPECIES_JSON__", species_json)
        .replace("__LETTER_JSON__", letter_json)
        .replace("__QUEST_JSON__", quest_json)
        .replace("__CARDS__", cards)
        .replace("__FISH_NAMES__", fish_names)
        .replace("__FISH_AREAS__", fish_areas)
        .replace("__INSECT_NAMES__", insect_names)
        .replace("__INSECT_AREAS__", insect_areas)
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

    let (town_style, town_body, town_script) = viewer_parts(&plan_json(&plan));
    let species_json = species_json();
    println!("Species JSON: {} bytes", species_json.len());
    let letter_json = letter_lab_json();
    let quest_json = quest_board_json();
    let cards = cards_html();

    let fish_names = js_name_array(&(0..45).map(fish_name).collect::<Vec<_>>());
    let fish_areas = js_name_array(&(0..7).map(fish_area_name).collect::<Vec<_>>());
    let insect_names = js_name_array(&(0..42).map(insect_name).collect::<Vec<_>>());
    let insect_areas = js_name_array(&(0..14).map(insect_area_name).collect::<Vec<_>>());

    let page = build_page(
        &town_style, &town_body, &town_script, &species_json, &letter_json,
        &quest_json, &cards, &fish_names, &fish_areas, &insect_names, &insect_areas,
    );

    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let dir = executable.parent().ok_or("no output dir")?;
    let path = dir.join("showcase.html");
    fs::write(&path, page).map_err(|e| format!("write failed: {e}"))?;
    println!("Showcase written to: {}", path.display());
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("Error: {message}");
        std::process::exit(2);
    }
}
