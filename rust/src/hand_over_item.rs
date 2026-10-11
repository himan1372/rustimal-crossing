//! Item hand-over animation between talkers (`ac_handOverItem.c` +
//! `_clip.c_inc` + `_move.c_inc`).
//!
//! Source-verified against ac-decomp (primary) and ACGC-PC-Port
//! (secondary: `anm_cnt` advances by `0.5 * dt_num_60fps_frames`).
//!
//! ## Protocol (`aHOI_Clip_c`)
//! - `birth_proc(item, after_mode, present_flag, master)`: only when no
//!   transfer is active and a talk pair exists; the target is whichever of
//!   speaker/listener is not the master. `after_mode` outside 5..=8 becomes
//!   7. Locks the message window from continuing.
//! - `chg_master_proc(actor)`: when the *target* calls during TRANS_WAIT or
//!   RETURN_WAIT, master and target swap and the mode becomes GET.
//! - `chg_request_mode_proc(master, mode)`: only the current master may
//!   change the mode.
//!
//! ## Animation
//! `anm_cnt` advances 0.5 per frame up to a per-mode maximum; translation
//! and scale are piecewise-linear over small keyframe tables chosen by
//! item kind: 0 normal item, 1 fish (held lower), 2 mail (fortune slips
//! and everything handed over in the post office). When the scale is 0
//! and the animation is at its end, the transfer finishes (master cleared,
//! mode NO_REQUEST, message lock released). Transfer plays SE 0x56 at
//! `anm_cnt > 24`.

/// `aHOI_REQUEST_*`.
pub const REQ_NO_REQUEST: usize = 0;
pub const REQ_TRANSFER: usize = 1;
pub const REQ_TRANS_WAIT: usize = 2;
pub const REQ_GET: usize = 3;
pub const REQ_GET_PULL: usize = 4;
pub const REQ_EAT: usize = 5;
pub const REQ_CHANGE: usize = 6;
pub const REQ_PUTAWAY: usize = 7;
pub const REQ_GET_PULL_WAIT: usize = 8;
pub const REQ_RETURN: usize = 9;
pub const REQ_RETURN_WAIT: usize = 10;
pub const REQ_ESTIMATE: usize = 11;
pub const REQ_NUM: usize = 12;

/// `aHOI_ITEM_TYPE_*`.
pub const ITEM_TYPE_ITEM: usize = 0;
pub const ITEM_TYPE_FISH: usize = 1;
pub const ITEM_TYPE_MAIL: usize = 2;

/// `F32_IS_ZERO(v)` is `fabsf(v) < 0.008f` (include/types.h), not machine epsilon.
pub const F32_ZERO_EPS: f32 = 0.008;

pub const TRANSFER_SE: u16 = 0x56;
pub const TRANSFER_SE_FRAME: f32 = 24.0;

type Key3 = (f32, [f32; 3]);

const NO_REQ: &[Key3] = &[(0.0, [0.0; 3]), (1.0, [0.0; 3])];
const TRANSFER: &[Key3] = &[(0.0, [9.0, 0.0, 0.0]), (17.0, [9.0, 0.0, 0.0]), (31.0, [14.0, -9.5, 6.0]), (39.0, [14.0, -9.5, 6.0])];
const TRANSFER_F: &[Key3] = &[(0.0, [9.0, 0.0, 0.0]), (17.0, [9.0, 0.0, 0.0]), (31.0, [14.0, -3.0, 1.0]), (39.0, [14.0, -3.0, 1.0])];
const TRANS_WAIT: &[Key3] = &[(0.0, [14.0, -9.5, 6.0]), (1.0, [14.0, -9.5, 6.0])];
const TRANS_WAIT_F: &[Key3] = &[(0.0, [14.0, -3.0, 1.0]), (1.0, [14.0, -3.0, 1.0])];
const EAT: &[Key3] = &[(0.0, [14.0, -3.0, 1.0]), (10.0, [14.0, -3.0, 1.0]), (28.0, [10.0, -0.5, 5.0]), (59.0, [10.0, -0.5, 5.0])];
const CHANGE: &[Key3] = &[(0.0, [14.0, -9.5, 6.0]), (8.0, [9.0, 0.0, 0.0]), (57.0, [9.0, 0.0, 0.0])];
const PUTAWAY: &[Key3] = &[(0.0, [14.0, -9.5, 6.0]), (13.0, [9.0, 0.0, 0.0]), (31.0, [9.0, 0.0, 0.0])];
const PUTAWAY_F: &[Key3] = &[(0.0, [14.0, -3.0, 1.0]), (13.0, [9.0, 0.0, 0.0]), (31.0, [9.0, 0.0, 0.0])];
const SEND_MAIL: &[Key3] = &[(0.0, [9.0, 0.0, 0.0]), (17.0, [9.0, 0.0, 0.0]), (38.0, [24.0, -3.0, 2.5])];
const SEND_MAIL_WAIT: &[Key3] = &[(0.0, [24.0, -3.0, 2.5]), (1.0, [24.0, -3.0, 2.5])];
const CONTRACT1: &[Key3] = &[(0.0, [31.5, -4.0, 2.0]), (1.0, [31.5, -4.0, 2.0])];
const CONTRACT2: &[Key3] = &[(0.0, [31.5, -4.0, 2.0]), (13.0, [17.5, -1.5, 0.75])];
const CONTRACT3: &[Key3] = &[(0.0, [17.5, -1.5, 0.75]), (5.0, [17.5, -1.5, 0.75]), (18.0, [31.5, -4.0, 2.0])];
const NOT_CONTRACT_WAIT: &[Key3] = &[(0.0, [31.5, -4.0, 2.0]), (1.0, [31.5, -4.0, 2.0])];

/// `trans_data_table{,_f,_m}`: [item kind][mode].
pub const TRANS_TABLES: [[&[Key3]; REQ_NUM]; 3] = [
    [NO_REQ, TRANSFER, TRANS_WAIT, TRANS_WAIT, TRANS_WAIT, EAT, CHANGE, PUTAWAY, TRANS_WAIT, TRANS_WAIT, TRANS_WAIT, TRANS_WAIT_F],
    [NO_REQ, TRANSFER_F, TRANS_WAIT_F, TRANS_WAIT_F, TRANS_WAIT_F, EAT, CHANGE, PUTAWAY_F, TRANS_WAIT_F, TRANS_WAIT_F, TRANS_WAIT_F, TRANS_WAIT_F],
    // not_contract_data is identical to contract3_data in the source.
    [NO_REQ, SEND_MAIL, SEND_MAIL_WAIT, CONTRACT1, CONTRACT2, EAT, CHANGE, CONTRACT3, TRANS_WAIT_F, CONTRACT3, NOT_CONTRACT_WAIT, TRANS_WAIT_F],
];

type Key1 = (f32, f32);
const S_NO_REQ: &[Key1] = &[(0.0, 0.0), (1.0, 0.0)];
const S_TRANSFER: &[Key1] = &[(0.0, 0.0), (17.0, 0.0), (35.0, 1.0)];
const S_WAIT: &[Key1] = &[(0.0, 1.0), (1.0, 1.0)];
const S_EAT: &[Key1] = &[(0.0, 1.0), (28.0, 0.0)];
const S_PUTAWAY: &[Key1] = &[(0.0, 1.0), (13.0, 0.0)];
const S_SEND_MAIL: &[Key1] = &[(0.0, 0.0), (38.0, 1.0)];
const S_CONTRACT3: &[Key1] = &[(0.0, 1.0), (5.0, 1.0), (18.0, 0.0)];

/// `scale_data_table{,_m}` (fish shares the item table).
pub const SCALE_TABLES: [[&[Key1]; REQ_NUM]; 3] = [
    [S_NO_REQ, S_TRANSFER, S_WAIT, S_WAIT, S_WAIT, S_EAT, S_EAT, S_PUTAWAY, S_WAIT, S_WAIT, S_WAIT, S_WAIT],
    [S_NO_REQ, S_TRANSFER, S_WAIT, S_WAIT, S_WAIT, S_EAT, S_EAT, S_PUTAWAY, S_WAIT, S_WAIT, S_WAIT, S_WAIT],
    [S_NO_REQ, S_SEND_MAIL, S_WAIT, S_WAIT, S_WAIT, S_EAT, S_EAT, S_CONTRACT3, S_WAIT, S_WAIT, S_WAIT, S_WAIT],
];

/// `max_anm_cnt{,_m}`.
pub const MAX_ANM: [[f32; REQ_NUM]; 3] = [
    [1.0, 39.0, 17.0, 11.0, 11.0, 59.0, 57.0, 31.0, 17.0, 11.0, 1.0, 81.0],
    [1.0, 39.0, 17.0, 11.0, 11.0, 59.0, 57.0, 31.0, 17.0, 11.0, 1.0, 81.0],
    [1.0, 38.0, 1.0, 12.0, 18.0, 59.0, 57.0, 29.0, 17.0, 29.0, 1.0, 81.0],
];

/// Segment selection shared by `aHOI_calc_trans` / `aHOI_calc_scale`:
/// returns (segment start index, clamped frame).
fn segment(frames: impl Fn(usize) -> f32, max_idx: usize, f: f32) -> (usize, f32) {
    let mut i = max_idx;
    while i != 0 {
        if f > frames(i) {
            break;
        }
        i -= 1;
    }
    if i >= max_idx {
        (max_idx - 1, frames(max_idx))
    } else {
        (i, f)
    }
}

/// `aHOI_calc_trans`.
pub fn calc_trans(item_type: usize, mode: usize, anm_cnt: f32) -> [f32; 3] {
    let keys = TRANS_TABLES[item_type][mode];
    let max_idx = keys.len() - 1;
    let (i, f) = segment(|k| keys[k].0, max_idx, anm_cnt);
    let (f0, a) = keys[i];
    let (f1, b) = keys[i + 1];
    let t = (f - f0) / (f1 - f0);
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]
}

/// `aHOI_calc_scale` (same arithmetic order as the C).
pub fn calc_scale(item_type: usize, mode: usize, anm_cnt: f32) -> f32 {
    let keys = SCALE_TABLES[item_type][mode];
    let max_idx = keys.len() - 1;
    let (i, f) = segment(|k| keys[k].0, max_idx, anm_cnt);
    let (f0, s0) = keys[i];
    let (f1, s1) = keys[i + 1];
    s0 + ((f - f0) * (s1 - s0)) / (f1 - f0)
}

/// `aHOI_anime_proc` counter step; returns (new anm_cnt, finished).
pub fn advance(item_type: usize, mode: usize, anm_cnt: f32, step: f32) -> (f32, f32, [f32; 3], bool) {
    let max = MAX_ANM[item_type][mode];
    let cnt = (anm_cnt + step).min(max);
    let scale = calc_scale(item_type, mode, cnt);
    let trans = calc_trans(item_type, mode, cnt);
    let finished = scale.abs() < F32_ZERO_EPS && cnt == max;
    (cnt, scale, trans, finished)
}

/// `aHOI_get_item_type`.
pub fn item_type(item: u16, fortune_slip: u16, in_post_office: bool) -> usize {
    const NAME_TYPE_ITEM1: u16 = 2;
    const ITEM1_CAT_FISH: u16 = 3;
    if item == fortune_slip || in_post_office {
        ITEM_TYPE_MAIL
    } else if (item & 0xF000) >> 12 == NAME_TYPE_ITEM1 && (item & 0x0F00) >> 8 == ITEM1_CAT_FISH {
        ITEM_TYPE_FISH
    } else {
        ITEM_TYPE_ITEM
    }
}

/// `aHOI_birth_proc` after-mode sanitising.
pub fn sanitize_after_mode(after_mode: i32) -> i32 {
    if (5..=8).contains(&after_mode) {
        after_mode
    } else {
        7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_curve() {
        assert_eq!(calc_trans(0, REQ_TRANSFER, 0.0), [9.0, 0.0, 0.0]);
        assert_eq!(calc_trans(0, REQ_TRANSFER, 39.0), [14.0, -9.5, 6.0]);
        let mid = calc_trans(0, REQ_TRANSFER, 24.0);
        assert!((mid[0] - 11.5).abs() < 1e-5);
        assert_eq!(calc_scale(0, REQ_TRANSFER, 10.0), 0.0);
        assert_eq!(calc_scale(0, REQ_TRANSFER, 35.0), 1.0);
    }

    #[test]
    fn eat_finishes() {
        let mut cnt = 0.0;
        let mut frames = 0;
        loop {
            let (c, _, _, done) = advance(0, REQ_EAT, cnt, 0.5);
            cnt = c;
            frames += 1;
            if done {
                break;
            }
            assert!(frames < 1000);
        }
        assert_eq!(frames, 118); // 59 / 0.5
    }

    #[test]
    fn item_kinds() {
        assert_eq!(item_type(0x2300, 0x2B00, false), ITEM_TYPE_FISH);
        assert_eq!(item_type(0x2300, 0x2B00, true), ITEM_TYPE_MAIL);
        assert_eq!(item_type(0x2100, 0x2B00, false), ITEM_TYPE_ITEM);
        assert_eq!(sanitize_after_mode(3), 7);
    }
}
