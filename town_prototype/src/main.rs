#[path = "../../rust/src/town_gen.rs"]
mod town_gen;

use std::env;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::PathBuf;
use std::process;
use town_gen::{generate, CellKind, Feature, TownAcre, ACRE_WIDTH, MAX_TOWN_VILLAGERS};

fn usage() {
    println!("Animal Crossing town placement prototype (32-bit Windows)");
    println!("Usage: ac_town_prototype.exe [--seed NUMBER] [--villagers 1..15]");
    println!("Example: ac_town_prototype.exe --seed 305419896 --villagers 6");
}

fn arguments() -> Result<(u32, usize), String> {
    let mut seed = 305419896u32;
    let mut villagers = 6usize;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                usage();
                process::exit(0);
            }
            "--seed" => {
                let value = args.next().ok_or("--seed requires a 32-bit unsigned number")?;
                seed = value
                    .parse()
                    .map_err(|_| "--seed must be a 32-bit unsigned number")?;
            }
            "--villagers" => {
                let value = args.next().ok_or("--villagers requires a number from 1 to 15")?;
                villagers = value
                    .parse()
                    .map_err(|_| "--villagers must be a number from 1 to 15")?;
                if !(1..=MAX_TOWN_VILLAGERS).contains(&villagers) {
                    return Err("--villagers must be a number from 1 to 15".to_owned());
                }
            }
            unknown => return Err(format!("unknown argument: {unknown}")),
        }
    }
    Ok((seed, villagers))
}

fn cell_symbol(cell: u8) -> char {
    match cell {
        x if x == CellKind::House as u8 => 'H',
        x if x == CellKind::HouseSign as u8 => 'S',
        x if x == CellKind::HouseReserved as u8 => '#',
        x if x == CellKind::Tree as u8 => 'T',
        x if x == CellKind::Flower as u8 => 'f',
        x if x == CellKind::Rock as u8 => 'R',
        x if x == CellKind::Weed as u8 => 'w',
        x if x == CellKind::Litter as u8 => 'x',
        _ => '.',
    }
}

fn feature_symbol(feature: u8) -> char {
    match feature {
        x if x == Feature::Station as u8 => 'S',
        x if x == Feature::Shop as u8 => 'M',
        x if x == Feature::PostOffice as u8 => 'P',
        x if x == Feature::PlayerHouse as u8 => 'p',
        x if x == Feature::WishingWell as u8 => 'W',
        x if x == Feature::PoliceStation as u8 => 'C',
        x if x == Feature::Museum as u8 => 'U',
        x if x == Feature::Tailor as u8 => 'A',
        x if x == Feature::Dock as u8 => 'D',
        _ => '.',
    }
}

fn resident_home_count(acre: &TownAcre) -> usize {
    acre.cells.iter().filter(|&&cell| cell == CellKind::House as u8).count()
}

fn print_acre_map(plan: &town_gen::TownPlan, acre_index: usize) {
    let acre = &plan.acres[acre_index];
    let x = acre_index % ACRE_WIDTH;
    let z = acre_index / ACRE_WIDTH;
    println!("\nHome acre ({x},{z}) - unit coordinates x=0..15, z=0..15:");
    for row in acre.cells.chunks(16) {
        for &cell in row {
            print!("{}", cell_symbol(cell));
        }
        println!();
    }
}

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

fn write_3d_preview(plan: &town_gen::TownPlan) -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable
        .parent()
        .ok_or("could not locate the prototype output directory")?;
    let path = directory.join("ac_town_preview.html");
    let template = include_str!("../viewer_template.html");
    let html = template.replace("__TOWN_DATA__", &plan_json(plan));
    fs::write(&path, html).map_err(|error| format!("could not write 3D preview: {error}"))?;
    Ok(path)
}

fn run() -> Result<(), String> {
    let (seed, villager_count) = arguments()?;
    let resident_ids: Vec<u16> = (0..villager_count).map(|i| 1000 + i as u16).collect();
    let (plan, used_seed) = (0..64u32)
        .find_map(|retry| {
            let candidate_seed = seed.wrapping_add(retry);
            generate(candidate_seed, &resident_ids, villager_count)
                .map(|plan| (plan, candidate_seed))
        })
        .ok_or("the Rust town planner could not create a valid town within 64 seed attempts")?;
    let preview_path = write_3d_preview(&plan)?;

    println!("Animal Crossing Rust town placement prototype");
    if used_seed == seed {
        println!("Seed: {} | residents: {}", used_seed, plan.villager_count);
    } else {
        println!(
            "Requested seed {} was not placeable; using seed {} | residents: {}",
            seed, used_seed, plan.villager_count
        );
    }
    println!("Town map (5 acres wide x 6 deep):");
    for z in 0..6 {
        for x in 0..5 {
            let index = z * ACRE_WIDTH + x;
            let acre = &plan.acres[index];
            let symbol = if acre.feature != Feature::None as u8 {
                feature_symbol(acre.feature)
            } else if resident_home_count(acre) > 0 {
                'v'
            } else {
                char::from(b'0' + acre.elevation)
            };
            print!("{symbol} ");
        }
        println!();
    }

    println!("\nResidents and home centers:");
    for i in 0..villager_count {
        let acre_index = plan.house_acres[i] as usize;
        let center = plan.house_units[i] as usize;
        println!(
            "  ID {} -> acre ({},{}), unit ({},{})",
            plan.villagers[i],
            acre_index % ACRE_WIDTH,
            acre_index / ACRE_WIDTH,
            center % 16,
            center / 16
        );
    }

    println!("\nAcre detail legend: H=house, S=sign, #=reserved footprint, .=open unit");
    let mut printed_acres = Vec::new();
    for i in 0..villager_count {
        let acre_index = plan.house_acres[i] as usize;
        if !printed_acres.contains(&acre_index) {
            print_acre_map(&plan, acre_index);
            printed_acres.push(acre_index);
        }
    }
    println!("\nFacility legend: S=station M=shop P=post office p=player house");
    println!("               W=well C=police U=museum A=tailor D=dock; digits=elevation");
    println!("\n3D preview written to: {}", preview_path.display());
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("Error: {message}");
        eprintln!("Use --help for usage.");
        process::exit(2);
    }
}
