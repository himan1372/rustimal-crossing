//! Rust implementations of selected PC runtime compatibility modules.
//!
//! Exported functions keep the existing C ABIs used by the PC port and game
//! decompilation. Disc buffers returned to C use the C runtime's `malloc`, so
//! existing callers may continue to release them with `free`.

#![allow(non_snake_case)]

use std::ffi::{c_char, c_void, CStr};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;
use std::sync::Mutex;

mod aram;
mod dvd;
mod game_time;
mod buried_items;
mod house;
mod shop;
mod behavior;
mod attr_walls;
mod bg_check;
mod columns;
mod decal_circles;
mod segment_map;
mod collision;
mod dialogue_topics;
mod attribute_action;
mod bridge_acre;
mod column_sweep;
mod endpoint_circle;
mod move_bg;
mod terrain_walls;
mod interaction;
mod inventory;
mod item_prefs;
mod movement;
mod player_move;
mod scene;
mod scene_layout;
mod gbi_runtime;
mod house_scene;
mod letter_score;
mod mtx;
mod npc;
mod profiler;
mod request_selector;
mod quest;
mod quest_gen;
mod ecology;
mod save;
mod town_gen;
mod vi;
mod villager_mail;
mod mail;
mod furniture;
mod wpos2attribute;
mod slate_classify;
mod talk_request;
mod talk_topics;
mod force_call;
mod npc_ai;
mod player_action;
mod player_tools;
mod field_gen;
mod villager_home;
mod fg_data;
mod scene_table;
mod weather_season;
mod tool_resolvers;
mod frame_loops;
mod collision_temporal;
mod albumin;
mod placement;
mod albumin_geometry;
mod albumin_collision_data;
mod template_select;
mod species;
mod uki;
mod bee_ant;
mod special_delivery;
mod leaflet;
mod mother_mail;
mod mck_key_tables;
mod npc_reply;
mod save_format;
mod fish_tables;
mod insect_tables;
mod step3_data;
mod wall_hit_dir;
mod wall_priority;
mod wall_solver;

const CISO_HEADER_SIZE: usize = 0x8000;
const CISO_MAP_OFFSET: usize = 8;
const CISO_MAGIC_LE: u32 = 0x4F53_4943;
const GC_MAGIC: u32 = 0xC233_9F3D;
const MAX_FST_FILES: usize = 1024;
const MAX_FST_ENTRIES: usize = 1_000_000;

#[derive(Clone)]
struct FstFile {
    path: String,
    disc_offset: u32,
    file_size: u32,
}

struct Disc {
    file: File,
    is_ciso: bool,
    block_size: u32,
    block_phys: Vec<Option<u32>>,
    dol_offset: u32,
    dol_size: u32,
    files: Vec<FstFile>,
}

static DISC: Mutex<Option<Disc>> = Mutex::new(None);

unsafe extern "C" {
    static mut g_pc_verbose: i32;
    fn malloc(size: usize) -> *mut c_void;
}

fn verbose() -> bool {
    // SAFETY: This global is defined and initialized by pc_main.c.
    unsafe { g_pc_verbose != 0 }
}

fn be32(bytes: &[u8]) -> Option<u32> {
    let b: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(u32::from_be_bytes(b))
}

fn le32(bytes: &[u8]) -> Option<u32> {
    let b: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

fn zeroed_bytes(len: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).ok()?;
    bytes.resize(len, 0);
    Some(bytes)
}

impl Disc {
    fn open(path: &Path) -> Option<Self> {
        let mut file = File::open(path).ok()?;
        let mut header = zeroed_bytes(CISO_HEADER_SIZE)?;
        let read_header = file.read_exact(&mut header).is_ok();
        let mut is_ciso = false;
        let mut block_size = 0;
        let mut block_phys = Vec::new();

        if read_header && le32(&header)? == CISO_MAGIC_LE {
            block_size = le32(&header[4..])?;
            if block_size == 0 {
                return None;
            }
            let mut physical = 0u32;
            block_phys.reserve(CISO_HEADER_SIZE - CISO_MAP_OFFSET);
            for &present in &header[CISO_MAP_OFFSET..] {
                if present != 0 {
                    block_phys.push(Some(physical));
                    physical = physical.checked_add(1)?;
                } else {
                    block_phys.push(None);
                }
            }
            is_ciso = true;
        }

        let mut disc = Self {
            file,
            is_ciso,
            block_size,
            block_phys,
            dol_offset: 0,
            dol_size: 0,
            files: Vec::new(),
        };

        let mut magic = [0; 4];
        if !disc.read_at(0x1C, &mut magic) || be32(&magic) != Some(GC_MAGIC) {
            return None;
        }

        let mut word = [0; 4];
        if disc.read_at(0x420, &mut word) {
            disc.dol_offset = be32(&word).unwrap_or(0);
            disc.dol_size = disc.calculate_dol_size();
        }
        disc.files = disc.read_fst();
        Some(disc)
    }

    fn read_at(&mut self, offset: u32, out: &mut [u8]) -> bool {
        if !self.is_ciso {
            return self.file.seek(SeekFrom::Start(offset as u64)).is_ok()
                && self.file.read_exact(out).is_ok();
        }

        if self.block_size == 0 {
            return false;
        }
        let mut logical = offset as u64;
        let mut written = 0usize;
        while written < out.len() {
            let block_index = (logical / self.block_size as u64) as usize;
            let within = (logical % self.block_size as u64) as usize;
            let chunk = (self.block_size as usize - within).min(out.len() - written);
            match self.block_phys.get(block_index).copied().flatten() {
                None => out[written..written + chunk].fill(0),
                Some(physical) => {
                    let physical_offset = CISO_HEADER_SIZE as u64
                        + physical as u64 * self.block_size as u64
                        + within as u64;
                    if self.file.seek(SeekFrom::Start(physical_offset)).is_err()
                        || self
                            .file
                            .read_exact(&mut out[written..written + chunk])
                            .is_err()
                    {
                        return false;
                    }
                }
            }
            logical += chunk as u64;
            written += chunk;
        }
        true
    }

    fn calculate_dol_size(&mut self) -> u32 {
        let mut header = [0u8; 0xE4];
        if !self.read_at(self.dol_offset, &mut header) {
            return 0;
        }
        let mut max_end = 0u32;
        for (off_start, size_start, count) in [(0x00usize, 0x90usize, 7usize), (0x1C, 0xAC, 11)] {
            for i in 0..count {
                let Some(offset) = be32(&header[off_start + i * 4..]) else {
                    return 0;
                };
                let Some(size) = be32(&header[size_start + i * 4..]) else {
                    return 0;
                };
                let Some(end) = offset.checked_add(size) else {
                    return 0;
                };
                max_end = max_end.max(end);
            }
        }
        max_end
    }

    fn read_fst(&mut self) -> Vec<FstFile> {
        let mut head = [0u8; 8];
        if !self.read_at(0x424, &mut head) {
            return Vec::new();
        }
        let Some(fst_offset) = be32(&head) else {
            return Vec::new();
        };
        let Some(fst_size) = be32(&head[4..]) else {
            return Vec::new();
        };
        if fst_size < 12 || fst_size > 128 * 1024 * 1024 {
            return Vec::new();
        }
        let Some(mut fst) = zeroed_bytes(fst_size as usize) else {
            return Vec::new();
        };
        if !self.read_at(fst_offset, &mut fst) {
            return Vec::new();
        }

        let Some(entry_count) = be32(fst.get(8..12).unwrap_or(&[])) else {
            return Vec::new();
        };
        let entry_count = entry_count as usize;
        if entry_count < 1 || entry_count > MAX_FST_ENTRIES {
            return Vec::new();
        }
        let Some(entries_size) = entry_count.checked_mul(12) else {
            return Vec::new();
        };
        if entries_size > fst.len() {
            return Vec::new();
        }
        let strings = &fst[entries_size..];
        let Some(root_end) = be32(&fst[8..12]).map(|n| n as usize) else {
            return Vec::new();
        };
        if root_end > entry_count {
            return Vec::new();
        }

        let mut dirs: Vec<(usize, String)> = vec![(root_end, String::new())];
        let mut files = Vec::new();
        for index in 1..entry_count {
            while dirs.last().is_some_and(|(end, _)| index >= *end) {
                dirs.pop();
            }
            let Some(node_start) = index.checked_mul(12) else {
                break;
            };
            let Some(node) = fst.get(node_start..node_start + 12) else {
                break;
            };
            let name_offset =
                ((node[1] as usize) << 16) | ((node[2] as usize) << 8) | node[3] as usize;
            let Some(name_bytes) = strings.get(name_offset..) else {
                continue;
            };
            let end = name_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
            if node[0] == 1 {
                let Some(dir_end) = be32(&node[8..]).map(|n| n as usize) else {
                    continue;
                };
                if dirs.len() < 32 && dir_end <= entry_count {
                    dirs.push((dir_end, name));
                }
            } else if files.len() < MAX_FST_FILES {
                let mut path = String::new();
                for (_, dir) in dirs.iter().skip(1) {
                    if !path.is_empty() {
                        path.push('/');
                    }
                    path.push_str(dir);
                }
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(&name);
                if let (Some(disc_offset), Some(file_size)) = (be32(&node[4..]), be32(&node[8..])) {
                    files.push(FstFile {
                        path,
                        disc_offset,
                        file_size,
                    });
                }
            }
        }
        files
    }
}

fn find_image() -> Option<PathBuf> {
    for dir in [Path::new("."), Path::new("orig"), Path::new("rom")] {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(extension) = path.extension() else {
                continue;
            };
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            if matches!(extension.as_str(), "ciso" | "iso" | "gcm") {
                return Some(path);
            }
        }
    }
    None
}

fn decode_yaz0(src: &[u8]) -> Option<Vec<u8>> {
    if src.len() < 16 || src.get(..4)? != b"Yaz0" {
        return None;
    }
    let out_len = be32(src.get(4..)?)? as usize;
    let mut dst = Vec::new();
    dst.try_reserve_exact(out_len).ok()?;
    dst.resize(out_len, 0);
    let (mut sp, mut dp) = (16usize, 0usize);
    while dp < out_len && sp < src.len() {
        let flags = src[sp];
        sp += 1;
        for bit in (0..8).rev() {
            if dp >= out_len {
                break;
            }
            let Some(&first) = src.get(sp) else {
                return None;
            };
            sp += 1;
            if flags & (1 << bit) != 0 {
                dst[dp] = first;
                dp += 1;
                continue;
            }
            let Some(&second) = src.get(sp) else {
                return None;
            };
            sp += 1;
            let distance = (((first as usize) & 0x0F) << 8) | second as usize;
            let mut length = (first >> 4) as usize;
            if length == 0 {
                length = *src.get(sp)? as usize + 0x12;
                sp += 1;
            } else {
                length += 2;
            }
            let mut reference = dp.checked_sub(distance + 1)?;
            for _ in 0..length.min(out_len - dp) {
                let value = *dst.get(reference)?;
                dst[dp] = value;
                dp += 1;
                reference += 1;
            }
        }
    }
    (dp == out_len).then_some(dst)
}

fn c_buffer(bytes: &[u8]) -> *mut u8 {
    // SAFETY: The C caller owns and frees the returned malloc allocation.
    unsafe {
        let ptr = malloc(bytes.len().max(1)) as *mut u8;
        if ptr.is_null() {
            return ptr;
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        ptr
    }
}

#[no_mangle]
pub extern "C" fn pc_disc_init() -> i32 {
    let mut guard = DISC.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return 1;
    }
    let Some(path) = find_image() else { return 0 };
    let Some(disc) = Disc::open(&path) else {
        if verbose() {
            eprintln!("[PC] {}: not a valid GC disc image", path.display());
        }
        return 0;
    };
    if verbose() {
        let mut id = [0u8; 6];
        let mut disc = disc;
        let _ = disc.read_at(0, &mut id);
        println!(
            "[PC] Disc image: {} ({}, {})",
            path.display(),
            if disc.is_ciso { "CISO" } else { "ISO/GCM" },
            String::from_utf8_lossy(&id)
        );
        println!("[PC] FST: {} files indexed", disc.files.len());
        *guard = Some(disc);
    } else {
        *guard = Some(disc);
    }
    1
}

#[no_mangle]
pub extern "C" fn pc_disc_is_open() -> i32 {
    DISC.lock().unwrap_or_else(|e| e.into_inner()).is_some() as i32
}

#[no_mangle]
pub unsafe extern "C" fn pc_disc_find_file(
    path: *const c_char,
    disc_offset: *mut u32,
    file_size: *mut u32,
) -> i32 {
    if path.is_null() || disc_offset.is_null() || file_size.is_null() {
        return 0;
    }
    // SAFETY: `path` is a NUL-terminated C string per this function's ABI.
    let Ok(path) = CStr::from_ptr(path).to_str() else {
        return 0;
    };
    let path = path.strip_prefix('/').unwrap_or(path);
    let guard = DISC.lock().unwrap_or_else(|e| e.into_inner());
    let Some(disc) = guard.as_ref() else { return 0 };
    let Some(found) = disc.files.iter().find(|entry| entry.path == path) else {
        return 0;
    };
    // SAFETY: Both output pointers were checked non-null above.
    unsafe {
        *disc_offset = found.disc_offset;
        *file_size = found.file_size;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn pc_disc_read(offset: u32, dest: *mut c_void, size: u32) -> i32 {
    if (dest.is_null() && size != 0) || size as usize > isize::MAX as usize {
        return 0;
    }
    let mut guard = DISC.lock().unwrap_or_else(|e| e.into_inner());
    let Some(disc) = guard.as_mut() else { return 0 };
    if size == 0 {
        return 1;
    }
    // SAFETY: The C API contract requires `dest` to point to `size` writable bytes.
    let out = unsafe { slice::from_raw_parts_mut(dest.cast::<u8>(), size as usize) };
    disc.read_at(offset, out) as i32
}

#[no_mangle]
pub extern "C" fn pc_disc_extract_dol() -> *mut u8 {
    let mut guard = DISC.lock().unwrap_or_else(|e| e.into_inner());
    let Some(disc) = guard.as_mut() else {
        return ptr::null_mut();
    };
    let Some(mut data) = zeroed_bytes(disc.dol_size as usize) else {
        return ptr::null_mut();
    };
    if disc.dol_size == 0 || !disc.read_at(disc.dol_offset, &mut data) {
        return ptr::null_mut();
    }
    if verbose() {
        println!(
            "[PC] DOL: {} bytes (offset 0x{:X})",
            disc.dol_size, disc.dol_offset
        );
    }
    c_buffer(&data)
}

#[no_mangle]
pub extern "C" fn pc_disc_extract_rel() -> *mut u8 {
    let mut guard = DISC.lock().unwrap_or_else(|e| e.into_inner());
    let Some(disc) = guard.as_mut() else {
        return ptr::null_mut();
    };
    let Some(entry) = disc
        .files
        .iter()
        .find(|entry| entry.path == "foresta.rel.szs")
        .cloned()
    else {
        if verbose() {
            println!("[PC] foresta.rel.szs not found in disc FST");
        }
        return ptr::null_mut();
    };
    let Some(mut raw) = zeroed_bytes(entry.file_size as usize) else {
        return ptr::null_mut();
    };
    if !disc.read_at(entry.disc_offset, &mut raw) {
        return ptr::null_mut();
    }
    let data = if raw.starts_with(b"Yaz0") {
        match decode_yaz0(&raw) {
            Some(decoded) => {
                if verbose() {
                    println!(
                        "[PC] REL: {} bytes (Yaz0: {} -> {})",
                        decoded.len(),
                        raw.len(),
                        decoded.len()
                    );
                }
                decoded
            }
            None => {
                if verbose() {
                    println!("[PC] Yaz0 decompression failed");
                }
                return ptr::null_mut();
            }
        }
    } else {
        if verbose() {
            println!("[PC] REL: {} bytes (raw)", raw.len());
        }
        raw
    };
    c_buffer(&data)
}

#[no_mangle]
pub extern "C" fn pc_disc_shutdown() {
    *DISC.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Quest/letter constants from `m_quest.h` shared with behavior.rs.
pub const LETTER_SCORE_BONUS: i8 = 3;   // mQst_LETTER_SCORE_BONUS
pub const LETTER_PRESENT_BONUS: i8 = 6; // mQst_LETTER_PRESENT_BONUS
pub fn quest_time_limit_days() -> u8 {
    28 // mQst_MAX_TIME_LIMIT_DAYS
}
