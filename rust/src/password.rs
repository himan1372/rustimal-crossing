//! Secret passwords (m_passwordMake_ovl.c, m_passwordChk_ovl.c,
//! m_editor_ovl.c password paths, m_mail_password_check.c) pure kernels.
//!
//! Verified against GAFE01_00 Rev 0 decomp. The overlays collect
//! fixed-size byte strings; the codec transforms a 21-byte payload
//! into 28 six-bit characters through a fixed pipeline; the
//! shopkeeper validates the decoded result. Only deterministic,
//! engine-free helpers are ported here.
//!
//! The following stay in C until Rust/C equivalence is proven:
//! overlay lifecycle/allocation, the shared editor state machine,
//! sounds, warning windows, transitions, font rendering, ARAM/card
//! I/O, player/town identity lookups, the item catalog and price
//! checks, inventory mutation, NPC dialogue/reward flow, RNG
//! (magazine path), and the `pswd_famicom_list` extern data.

#![allow(non_snake_case)]

use std::os::raw::{c_int, c_uchar};

/// Password payload size (`mMpswd_PASSWORD_DATA_LEN`).
pub const PW_DATA_LEN: i32 = 21;
/// Encoded password string size (`mMpswd_PASSWORD_STR_LEN`).
pub const PW_STR_LEN: i32 = 28;
/// Key byte skipped by the bit stages (`mMpswd_PASSWORD_KEY_IDX`).
pub const PW_KEY_IDX: i32 = 1;
/// Nibble-select byte (`mMpswd_BITMIX_CODE_IDX`).
pub const PW_BITMIX_CODE_IDX: i32 = 1;
/// RSA r-prime byte (`mMpswd_RSA_R_PRIME_IDX`).
pub const PW_RSA_R_PRIME_IDX: i32 = 5;
/// RSA info byte (`mMpswd_RSA_INFO_IDX`).
pub const PW_RSA_INFO_IDX: i32 = 15;
/// RSA 9th-bit keysave byte (`mMpswd_RSA_KEYSAVE_IDX`).
pub const PW_RSA_KEYSAVE_IDX: i32 = 20;
/// Password line size (`mPC_STR_SIZE`).
pub const PW_LINE_SIZE: i32 = 14;
/// Password line count (`mPC_LINE_COUNT`).
pub const PW_LINE_COUNT: i32 = 2;
/// Town-name field size (`LAND_NAME_SIZE`).
pub const PW_TOWN_SIZE: i32 = 8;
/// Player-name field size (`PLAYER_NAME_LEN`).
pub const PW_PLAYER_SIZE: i32 = 8;
/// Game space character.
pub const PW_CHAR_SPACE: u8 = 0x20;
/// Explicit newline character.
pub const PW_CHAR_NEW_LINE: u8 = 205;
/// Password mail-body opening mark (`CHAR_SYMBOL_KEY`).
pub const PW_CHAR_SYMBOL_KEY: u8 = 212;

/// Password code types (`m_mail_password_check.h`), in order.
pub const PW_TYPE_FAMICOM: i32 = 0;
pub const PW_TYPE_POPULAR: i32 = 1;
pub const PW_TYPE_CARD_E: i32 = 2;
pub const PW_TYPE_MAGAZINE: i32 = 3;
pub const PW_TYPE_USER: i32 = 4;
pub const PW_TYPE_CARD_E_MINI: i32 = 5;
pub const PW_TYPE_NUM: i32 = 6;

/// Magazine hit-rate table (`hit_rate_magazine`,
/// `ac_npc_shop_common.c`), f32. The `RANDOM_F(100.0)` draw happens
/// only in the magazine validation path, guarded by
/// `hit_rate_index <= 4`; it stays in C with the RNG.
pub const PW_MAGAZINE_HIT_RATE: [f32; 5] = [80.0, 60.0, 30.0, 0.0, 100.0];

/// Prime table (`mMpswd_prime_number`, 256 entries, verbatim).
const PW_PRIMES: [u16; 256] = [
    17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61,
    67, 71, 73, 79, 83, 89, 97, 101, 103, 107, 109, 113,
    127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181,
    191, 193, 197, 199, 211, 223, 227, 229, 233, 239, 241, 251,
    257, 263, 269, 271, 277, 281, 283, 293, 307, 311, 313, 317,
    331, 337, 347, 349, 353, 359, 367, 373, 379, 383, 389, 397,
    401, 409, 419, 421, 431, 433, 439, 443, 449, 457, 461, 463,
    467, 479, 487, 491, 499, 503, 509, 521, 523, 541, 547, 557,
    563, 569, 571, 577, 587, 593, 599, 601, 607, 613, 617, 619,
    631, 641, 643, 647, 653, 659, 661, 673, 677, 683, 691, 701,
    709, 719, 727, 733, 739, 743, 751, 757, 761, 769, 773, 787,
    797, 809, 811, 821, 823, 827, 829, 839, 853, 857, 859, 863,
    877, 881, 883, 887, 907, 911, 919, 929, 937, 941, 947, 953,
    967, 971, 977, 983, 991, 997, 1009, 1013, 1019, 1021, 1031, 1033,
    1039, 1049, 1051, 1061, 1063, 1069, 1087, 1091, 1093, 1097, 1103, 1109,
    1117, 1123, 1129, 1151, 1153, 1163, 1171, 1181, 1187, 1193, 1201, 1213,
    1217, 1223, 1229, 1231, 1237, 1249, 1259, 1277, 1279, 1283, 1289, 1291,
    1297, 1301, 1303, 1307, 1319, 1321, 1327, 1361, 1367, 1373, 1381, 1399,
    1409, 1423, 1427, 1429, 1433, 1439, 1447, 1451, 1453, 1459, 1471, 1481,
    1483, 1487, 1489, 1493, 1499, 1511, 1523, 1531, 1543, 1549, 1553, 1559,
    1567, 1571, 1579, 1583, 1597, 1601, 1607, 1609, 1613, 1619, 1621, 1627,
    1637, 1657, 1663, 1667,
];

/// Substitution table (`mMpswd_chg_code_table`, 256 entries, verbatim).
const PW_SUBST: [u8; 256] = [
    0xf0, 0x83, 0xfd, 0x62, 0x93, 0x49, 0x0d, 0x3e,
    0xe1, 0xa4, 0x2b, 0xaf, 0x3a, 0x25, 0xd0, 0x82,
    0x7f, 0x97, 0xd2, 0x03, 0xb2, 0x32, 0xb4, 0xe6,
    0x09, 0x42, 0x57, 0x27, 0x60, 0xea, 0x76, 0xab,
    0x2d, 0x65, 0xa8, 0x4d, 0x8b, 0x95, 0x01, 0x37,
    0x59, 0x79, 0x33, 0xac, 0x2f, 0xae, 0x9f, 0xfe,
    0x56, 0xd9, 0x04, 0xc6, 0xb9, 0x28, 0x06, 0x5c,
    0x54, 0x8d, 0xe5, 0x00, 0xb3, 0x7b, 0x5e, 0xa7,
    0x3c, 0x78, 0xcb, 0x2e, 0x6d, 0xe4, 0xe8, 0xdc,
    0x40, 0xa0, 0xde, 0x2c, 0xf5, 0x1f, 0xcc, 0x85,
    0x71, 0x3d, 0x26, 0x74, 0x9c, 0x13, 0x7d, 0x7e,
    0x66, 0xf2, 0x9e, 0x02, 0xa1, 0x53, 0x15, 0x4f,
    0x51, 0x20, 0xd5, 0x39, 0x1a, 0x67, 0x99, 0x41,
    0xc7, 0xc3, 0xa6, 0xc4, 0xbc, 0x38, 0x8c, 0xaa,
    0x81, 0x12, 0xdd, 0x17, 0xb7, 0xef, 0x2a, 0x80,
    0x9d, 0x50, 0xdf, 0xcf, 0x89, 0xc8, 0x91, 0x1b,
    0xbb, 0x73, 0xf8, 0x14, 0x61, 0xc2, 0x45, 0xc5,
    0x55, 0xfc, 0x8e, 0xe9, 0x8a, 0x46, 0xdb, 0x4e,
    0x05, 0xc1, 0x64, 0xd1, 0xe0, 0x70, 0x16, 0xf9,
    0xb6, 0x36, 0x44, 0x8f, 0x0c, 0x29, 0xd3, 0x0e,
    0x6f, 0x7c, 0xd7, 0x4a, 0xff, 0x75, 0x6c, 0x11,
    0x10, 0x77, 0x3b, 0x98, 0xba, 0x69, 0x5b, 0xa3,
    0x6a, 0x72, 0x94, 0xd6, 0xd4, 0x22, 0x08, 0x86,
    0x31, 0x47, 0xbe, 0x87, 0x63, 0x34, 0x52, 0x3f,
    0x68, 0xf6, 0x0f, 0xbf, 0xeb, 0xc0, 0xce, 0x24,
    0xa5, 0x9a, 0x90, 0xed, 0x19, 0xb8, 0xb5, 0x96,
    0xfa, 0x88, 0x6e, 0xfb, 0x84, 0x23, 0x5d, 0xcd,
    0xee, 0x92, 0x58, 0x4c, 0x0b, 0xf7, 0x0a, 0xb1,
    0xda, 0x35, 0x5f, 0x9b, 0xc9, 0xa9, 0xe7, 0x07,
    0x1d, 0x18, 0xf3, 0xe3, 0xf1, 0xf4, 0xca, 0xb0,
    0x6b, 0x30, 0xec, 0x4b, 0x48, 0x1c, 0xad, 0xe2,
    0x21, 0x1e, 0xa2, 0xbd, 0x5a, 0xd8, 0x43, 0x7a,
];
/// Bit-selection index tables (`mMpswd_select_idx0..15`, verbatim).
const PW_SELECT: [[u8; 8]; 16] = [
    [17, 11,  0, 10, 12,  6,  8,  4],
    [ 3,  8, 11, 16,  4,  6,  9, 19],
    [ 9, 14, 17, 18, 11, 10, 12,  2],
    [ 0,  2,  1,  4, 18, 10, 12,  8],
    [17, 19, 16,  7, 12,  8,  2,  9],
    [16,  3,  1,  8, 18,  4,  7,  6],
    [19,  6, 10, 17,  3, 16,  8,  9],
    [17,  7, 18, 16, 12,  2, 11,  0],
    [ 6,  2, 12,  1,  8, 14,  0, 16],
    [19, 16, 11,  8, 17,  3,  6, 14],
    [18, 12,  2,  7, 10, 11,  1, 14],
    [ 8,  0, 14,  2,  7, 11, 12, 17],
    [ 9,  3,  2,  0, 11,  8, 14, 10],
    [10, 11, 12, 16, 19,  7, 17,  8],
    [19,  8,  6,  1, 17,  9, 14, 10],
    [ 9,  7, 17, 12, 19, 10,  1, 11],
];

/// Transposition strings, table 0 (NUL-inclusive lengths, verbatim).
const PW_TRANSPOSE_0: [(&[u8], usize); 16] = [
    (b"NiiMasaru\0", 9),
    (b"KomatsuKunihiro\0", 15),
    (b"TakakiGentarou\0", 14),
    (b"MiyakeHiromichi\0", 15),
    (b"HayakawaKenzo\0", 13),
    (b"KasamatsuShigehiro\0", 18),
    (b"SumiyoshiNobuhiro\0", 17),
    (b"NomaTakafumi\0", 12),
    (b"EguchiKatsuya\0", 13),
    (b"NogamiHisashi\0", 13),
    (b"IidaToki\0", 8),
    (b"IkegawaNoriko\0", 13),
    (b"KawaseTomohiro\0", 14),
    (b"BandoTaro\0", 9),
    (b"TotakaKazuo\0", 11),
    (b"WatanabeKunio\0", 13),
];

/// Transposition strings, table 1 (NUL-inclusive lengths, verbatim).
const PW_TRANSPOSE_1: [(&[u8], usize); 16] = [
    (b"RichAmtower\0", 11),
    (b"KyleHudson\0", 10),
    (b"MichaelKelbaugh\0", 15),
    (b"RaycholeLAneff\0", 14),
    (b"LeslieSwan\0", 10),
    (b"YoshinobuMantani\0", 16),
    (b"KirkBuchanan\0", 12),
    (b"TimOLeary\0", 9),
    (b"BillTrinen\0", 10),
    (b"nAkAyOsInoNyuuSankin\0", 20),
    (b"zendamaKINAKUDAMAkin\0", 20),
    (b"OishikutetUYOKUNARU\0", 19),
    (b"AsetoAminofen\0", 13),
    (b"fcSFCn64GCgbCGBagbVB\0", 20),
    (b"YossyIsland\0", 11),
    (b"KedamonoNoMori\0", 14),
];


/// Symbol -> game font code (`usable_to_fontnum`, 64 entries,
/// verbatim, byte values resolved from `m_font.h`).
const PW_FONTNUM: [u8; 64] = [
    98, 75, 122, 53, 99, 113, 89, 90, 79, 100, 116, 54, 110, 108, 66, 121,
    111, 56, 52, 76, 107, 37, 65, 81, 109, 68, 80, 73, 55, 38, 82, 115,
    119, 85, 209, 114, 51, 69, 120, 77, 67, 64, 101, 57, 103, 118, 86, 71,
    117, 78, 105, 88, 87, 102, 84, 74, 70, 83, 72, 112, 50, 97, 106, 104,
];

/// Allowed password characters (`usable_str_table` in
/// `mED_check_pw_usable_letter_sub`, verbatim): A-Z, a-z, `%`,
/// `&`, 0xD1, `@`, digits 2-9. Digits 0/1 are normalized to
/// `O`/`l` before the check; space is NOT allowed.
const PW_USABLE_LETTERS: [u8; 64] = [
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50,
    0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
    0x59, 0x5a, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
    0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e,
    0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76,
    0x77, 0x78, 0x79, 0x7a, 0x25, 0x26, 0xd1, 0x40,
    0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
];

/* ------------------------------------------------------------------ */
/* bit stages (verbatim ports of the // @fakematch routines)           */
/* ------------------------------------------------------------------ */

fn pw_bit_shift(d: &mut [u8; 21], shift: i32) {
    let mut buf = [0u8; 20];
    let mut shifted = [0u8; 20];
    buf[0] = d[0];
    buf[1..20].copy_from_slice(&d[2..21]);

    // this has to be a fakematch
    let mut bytes_shifted = shift.abs();
    bytes_shifted /= 8; // degenerate code
    let bits_shifted = shift.abs() % 8;
    while bytes_shifted > 20 {
        bytes_shifted -= 20;
    }

    if shift > 0 {
        let mut shift_mask: i32 = 0;
        for _ in 0..bits_shifted {
            shift_mask = shift_mask * 2 + 1;
        }
        let mut byte: i32 =
            ((buf[if bits_shifted == 0 { 0 } else { 19 }] as i32) >> (8 - bits_shifted)) & shift_mask;
        let mut dst = bytes_shifted as usize;
        for i in 0..20 {
            let now_byte = buf[i] as i32;
            shifted[dst] = (byte | (now_byte << bits_shifted)) as u8;
            byte = (now_byte >> (8 - bits_shifted)) & shift_mask;
            dst = (dst + 1) % 20;
        }
    } else if shift < 0 {
        let mut shift_mask: i32 = 0;
        for _ in 0..bits_shifted {
            shift_mask = shift_mask * 2 + 1;
        }
        // fakematch?
        let mut byte: i32 = buf[if bits_shifted == 0 { 19 } else { 0 }] as i32; // start byte
        let sh = 8 - bits_shifted; // invert shift
        // (mMpswd_PASSWORD_DATA_LEN - 2 - bytes_shifted) % (mMpswd_PASSWORD_DATA_LEN - 1)
        let mut shift_idx = (19 - bytes_shifted).rem_euclid(20) as usize; // dst index
        byte &= shift_mask;
        byte <<= sh; // move bits into position
        for i in (0..20).rev() {
            let now_byte = buf[i] as i32;
            shifted[shift_idx] = (byte | (now_byte >> bits_shifted)) as u8; // save shifted byte
            byte = (now_byte & shift_mask) << sh; // update initial bits
            if shift_idx == 0 {
                shift_idx = 19;
            } else {
                shift_idx -= 1;
            }
        }
    } else {
        shifted.copy_from_slice(&buf);
    }

    d[0] = shifted[0];
    d[2..21].copy_from_slice(&shifted[1..20]);
}

fn pw_bit_reverse(d: &mut [u8; 21]) {
    for i in 0..21 {
        if i != PW_KEY_IDX as usize {
            d[i] ^= 0xFF;
        }
    }
}

fn pw_bit_arrange_reverse(d: &mut [u8; 21]) {
    let mut buf = [0u8; 20];
    let mut reversed_buf = [0u8; 20];
    buf[0] = d[0];
    buf[1..20].copy_from_slice(&d[2..21]);
    let mut j = 0;
    for i in (0..20).rev() {
        let mut reversed = 0u8;
        for b in (0..8).rev() {
            reversed |= ((buf[i] >> b) & 1) << (7 - b);
        }
        reversed_buf[j] = reversed;
        j += 1;
    }
    d[0] = reversed_buf[0];
    d[2..21].copy_from_slice(&reversed_buf[1..20]);
}

fn pw_bit_shuffle(d: &mut [u8; 21], stage: i32) {
    let (key_idx, count) = if stage == 0 { (13usize, 19usize) } else { (2usize, 20usize) };
    let mut buf = [0u8; 20];
    let mut work_buf = [0u8; 20];
    buf[..key_idx].copy_from_slice(&d[..key_idx]);
    buf[key_idx..20].copy_from_slice(&d[key_idx + 1..21]);
    // bzero(work_buf, count); bytes at/above `count` stay zeroed.
    // (In C only `count` bytes are zeroed and index `count` is never
    // written; the byte is overwritten by the RSA stage right after,
    // so zeroing is behaviorally equivalent for every observable output.)
    let select = PW_SELECT[(d[key_idx] & 3) as usize];
    for i in 0..count {
        let b = buf[i];
        for bit in 0..8u8 {
            let mut dst_idx = i + select[bit as usize] as usize;
            if dst_idx >= count {
                dst_idx -= count;
            }
            work_buf[dst_idx] |= ((b >> bit) & 1) << bit;
        }
    }
    d[..key_idx].copy_from_slice(&work_buf[..key_idx]);
    d[key_idx + 1..21].copy_from_slice(&work_buf[key_idx..20]);
}

fn pw_decode_bit_shuffle(d: &mut [u8; 21], stage: i32) {
    let (key_idx, count) = if stage == 0 { (13usize, 19usize) } else { (2usize, 20usize) };
    let mut work_buf = [0u8; 20];
    let mut buf = [0u8; 20];
    work_buf[..key_idx].copy_from_slice(&d[..key_idx]);
    work_buf[key_idx..20].copy_from_slice(&d[key_idx + 1..21]);
    // bzero(buf, count); same uninitialized-byte note as above — byte
    // 20 is never observed after the full decode (unpack and the
    // checksum never read it), so zeroing is equivalent.
    let select = PW_SELECT[(d[key_idx] & 3) as usize];
    for i in 0..count {
        for bit in 0..8u8 {
            let mut dst_idx = i + select[bit as usize] as usize;
            if dst_idx >= count {
                dst_idx -= count;
            }
            buf[i] |= ((work_buf[dst_idx] >> bit) & 1) << bit;
        }
    }
    d[..key_idx].copy_from_slice(&buf[..key_idx]);
    d[key_idx + 1..21].copy_from_slice(&buf[key_idx..20]);
}

fn pw_rsa_key_code(d: &[u8; 21]) -> (i32, i32, i32, [u8; 8]) {
    let rsa_info = d[PW_RSA_INFO_IDX as usize] as u32;
    let mut p_idx = (rsa_info & 3) as i32;
    let mut q_idx = ((rsa_info >> 2) & 3) as i32;
    if p_idx == 3 {
        p_idx = (p_idx ^ q_idx) & 3;
        if p_idx == 3 {
            p_idx = 0;
        }
    }
    if q_idx == 3 {
        q_idx = (p_idx + 1) & 3;
        if q_idx == 3 {
            q_idx = 1;
        }
    }
    if p_idx == q_idx {
        q_idx = (p_idx + 1) & 3;
        if q_idx == 3 {
            q_idx = 1;
        }
    }
    let p = PW_PRIMES[p_idx as usize] as i32;
    let q = PW_PRIMES[q_idx as usize] as i32;
    let select = PW_SELECT[((rsa_info >> 4) & 0xF) as usize];
    let r = PW_PRIMES[d[PW_RSA_R_PRIME_IDX as usize] as usize] as i32;
    (p, q, r, select)
}

fn pw_rsa_encode(d: &mut [u8; 21]) {
    let (p, q, r, select) = pw_rsa_key_code(d);
    let mut rsa_keysave = 0i32;
    let pq = p * q;
    for i in 0..8 {
        let idx = select[i] as usize;
        let b = d[idx] as i32;
        let mut chg = b;
        let mut pow = r - 1;
        while pow > 0 {
            chg = (chg * b) % pq;
            pow -= 1;
        }
        d[idx] = chg as u8;
        // save the 9th bit state
        rsa_keysave |= ((chg >> 8) & 1) << i;
    }
    d[PW_RSA_KEYSAVE_IDX as usize] = rsa_keysave as u8;
}

fn pw_rsa_decode(d: &mut [u8; 21]) {
    let (p, q, r, select) = pw_rsa_key_code(d);
    let pq = p * q;
    let pq_1 = (p - 1) * (q - 1);
    // Calculate exponent d for decryption
    let mut r = r;
    let mut pow = 1i32;
    loop {
        let n = pow * pq_1 + 1;
        if n % r == 0 {
            r = n / r;
            break;
        }
        pow += 1;
    }
    let rsa_keysave = d[PW_RSA_KEYSAVE_IDX as usize] as i32;
    for i in 0..8 {
        let idx = select[i] as usize;
        let b = d[idx] as i32 | (((rsa_keysave >> i) & 1) << 8);
        let mut decrypted = b;
        let mut dd = r - 1;
        while dd > 0 {
            decrypted = (decrypted * b) % pq;
            dd -= 1;
        }
        d[idx] = decrypted as u8;
    }
}

fn pw_bit_mix(d: &mut [u8; 21]) {
    let code = (d[PW_BITMIX_CODE_IDX as usize] & 0xF) as i32;
    if code > 12 {
        pw_bit_arrange_reverse(d);
        pw_bit_reverse(d);
        pw_bit_shift(d, code * 3);
    } else if code > 8 {
        pw_bit_arrange_reverse(d);
        pw_bit_shift(d, -code * 5);
    } else if code > 4 {
        pw_bit_shift(d, -code * 5);
        pw_bit_reverse(d);
    } else {
        pw_bit_shift(d, code * 3);
        pw_bit_arrange_reverse(d);
    }
}

fn pw_decode_bit_mix(d: &mut [u8; 21]) {
    let code = (d[PW_BITMIX_CODE_IDX as usize] & 0xF) as i32;
    if code > 12 {
        pw_bit_shift(d, -code * 3);
        pw_bit_reverse(d);
        pw_bit_arrange_reverse(d);
    } else if code > 8 {
        pw_bit_shift(d, code * 5);
        pw_bit_arrange_reverse(d);
    } else if code > 4 {
        pw_bit_reverse(d);
        pw_bit_shift(d, code * 5);
    } else {
        pw_bit_arrange_reverse(d);
        pw_bit_shift(d, -code * 3);
    }
}

fn pw_substitution(d: &mut [u8; 21]) {
    for b in d.iter_mut() {
        *b = PW_SUBST[*b as usize];
    }
}

fn pw_decode_substitution(d: &mut [u8; 21]) {
    for i in 0..21 {
        for j in 0..256 {
            if PW_SUBST[j] == d[i] {
                d[i] = j as u8;
                break;
            }
        }
    }
}

fn pw_transpose(d: &mut [u8; 21], dir: i32, char_idx: usize) {
    const KEY_IDX: [usize; 2] = [18, 9];
    let key = KEY_IDX[char_idx];
    let tables = [PW_TRANSPOSE_0, PW_TRANSPOSE_1];
    let (s, len) = tables[char_idx][(d[key] & 0xF) as usize];
    let m: i32 = if dir == 1 { -1 } else { 1 };
    let mut str_idx = 0usize;
    for i in 0..21 {
        if i != key {
            d[i] = d[i].wrapping_add(((s[str_idx] as i32) * m) as u8);
            str_idx += 1;
            if str_idx >= len {
                str_idx = 0;
            }
        }
    }
}

fn pw_6bit_encode(six: &mut [u8; 28], eight: &[u8; 21]) {
    let mut six_idx = 0usize;
    let mut eight_bit = 0u32;
    let mut six_bit = 0u32;
    let mut six_byte = 0u8;
    let mut ei = 0usize;
    loop {
        let bit = ((eight[ei] >> eight_bit) & 1) << six_bit;
        eight_bit += 1;
        six_bit += 1;
        six_byte |= bit;
        if six_bit >= 6 {
            six_idx += 1;
            six[six_idx - 1] = six_byte;
            six_bit = 0;
            if six_idx >= PW_STR_LEN as usize {
                return;
            }
            six_byte = 0;
        }
        if eight_bit >= 8 {
            eight_bit = 0;
            ei += 1;
        }
    }
}

fn pw_8bit_decode(eight: &mut [u8; 21], six: &[u8; 28]) {
    let mut eight_idx = 0usize;
    let mut six_bit = 0u32;
    let mut eight_bit = 0u32;
    let mut eight_byte = 0u8;
    let mut si = 0usize;
    loop {
        let bit = ((six[si] >> six_bit) & 1) << eight_bit;
        six_bit += 1;
        eight_bit += 1;
        eight_byte |= bit;
        if eight_bit >= 8 {
            eight_idx += 1;
            eight[eight_idx - 1] = eight_byte;
            eight_bit = 0;
            if eight_idx >= PW_DATA_LEN as usize {
                return;
            }
            eight_byte = 0;
        }
        if six_bit >= 6 {
            six_bit = 0;
            si += 1;
        }
    }
}

fn pw_font_encode(s: &mut [u8; 28]) {
    let mut out = [0u8; 28];
    for i in 0..28 {
        out[i] = PW_FONTNUM[s[i] as usize];
    }
    s.copy_from_slice(&out);
}

fn pw_font_decode_sub(c: u8) -> u8 {
    let mut res = 0xFFu8;
    for i in 0..64 {
        if c == PW_FONTNUM[i] {
            res = i as u8; // probably should've used a break; here
        }
    }
    res
}

fn pw_font_decode(s: &mut [u8; 28]) -> bool {
    let mut out = [0u8; 28];
    for i in 0..28 {
        let n = pw_font_decode_sub(s[i]);
        if n == 0xFF {
            return false;
        }
        out[i] = n;
    }
    s.copy_from_slice(&out);
    true
}

fn pw_adjust_letter(s: &mut [u8; 28]) {
    for c in s.iter_mut() {
        match *c {
            48 => *c = 79,  // CHAR_ZERO -> CHAR_O
            49 => *c = 108, // CHAR_ONE -> CHAR_l
            _ => {}
        }
    }
}

fn pw_checksum(str0: &[u8; 8], str1: &[u8; 8], item: u16, npc_code: i32) -> i32 {
    let mut checksum: i32 = 0;
    for i in 0..8 {
        checksum += str0[i] as i32;
    }
    for i in 0..8 {
        checksum += str1[i] as i32;
    }
    checksum += item as i32;
    checksum += npc_code;
    checksum & 3
}

fn pw_make_passcode(
    out: &mut [u8; 21],
    type_: i32,
    hit_rate: i32,
    str0: &[u8; 8],
    str1: &[u8; 8],
    item: u16,
    npc_type: i32,
    npc_code: i32,
) {
    let (mut type_, mut hit_rate, mut npc_type, mut npc_code) = (type_, hit_rate, npc_type, npc_code);
    match type_ {
        PW_TYPE_FAMICOM | PW_TYPE_USER | PW_TYPE_CARD_E_MINI => {
            hit_rate = 1;
            npc_code = 0xFF;
        }
        PW_TYPE_POPULAR => {
            hit_rate = 4; // potential bug? when code is set to popularity contest, hitrate interferes with checksum bits
        }
        PW_TYPE_CARD_E => {}
        PW_TYPE_MAGAZINE => {
            npc_type = (hit_rate >> 2) & 1;
            hit_rate &= 3;
            npc_code = 0xFF;
        }
        _ => {
            type_ = PW_TYPE_USER;
        }
    }

    out[0] = (type_ << 5) as u8;
    out[0] |= (hit_rate << 1) as u8;
    out[0] |= (npc_type & 1) as u8;
    out[1] = npc_code as u8;
    out[2..10].copy_from_slice(str0);
    out[10..18].copy_from_slice(str1);
    out[18] = (item >> 8) as u8;
    out[19] = item as u8;
    out[0] |= ((pw_checksum(str0, str1, item, npc_code) & 3) << 3) as u8;
    out[20] = 0;
}

/* ------------------------------------------------------------------ */
/* C ABI                                                               */
/* ------------------------------------------------------------------ */

/// `mMpswd_make_password` (`m_mail_password_check.c`): builds the
/// 21-byte payload and runs the full encode pipeline, writing the
/// 28-character code to `out`. `str0`/`str1` are 8 bytes each
/// (recipient name, town name in the shop send path).
#[no_mangle]
pub extern "C" fn pc_pw_make_password(
    type_: c_int,
    hit_rate: c_int,
    str0: *const c_uchar,
    str1: *const c_uchar,
    item: u16,
    npc_type: c_int,
    npc_code: c_int,
    out: *mut c_uchar,
) {
    if str0.is_null() || str1.is_null() || out.is_null() {
        return;
    }
    let s0: [u8; 8] = unsafe { *(str0 as *const [u8; 8]) };
    let s1: [u8; 8] = unsafe { *(str1 as *const [u8; 8]) };
    let mut work = [0u8; 21];
    pw_make_passcode(&mut work, type_, hit_rate, &s0, &s1, item, npc_type, npc_code);
    pw_substitution(&mut work);
    pw_transpose(&mut work, 1, 0);
    pw_bit_shuffle(&mut work, 0);
    pw_rsa_encode(&mut work);
    pw_bit_mix(&mut work);
    pw_bit_shuffle(&mut work, 1);
    pw_transpose(&mut work, 0, 1);
    let mut six = [0u8; 28];
    pw_6bit_encode(&mut six, &work);
    pw_font_encode(&mut six);
    unsafe {
        std::ptr::copy_nonoverlapping(six.as_ptr(), out, 28);
    }
}

/// `mMpswd_decode_code` (`m_mail_password_check.c`): decodes a
/// 28-character code into the 21-byte payload. Returns 1 on
/// success, 0 when a character is outside the password alphabet.
#[no_mangle]
pub extern "C" fn pc_pw_decode_code(data_out: *mut c_uchar, str_in: *const c_uchar) -> c_int {
    if data_out.is_null() || str_in.is_null() {
        return 0;
    }
    let mut work = [0u8; 28];
    unsafe {
        std::ptr::copy_nonoverlapping(str_in, work.as_mut_ptr(), 28);
    }
    pw_adjust_letter(&mut work);
    if !pw_font_decode(&mut work) {
        return 0;
    }
    let mut data = [0u8; 21];
    pw_8bit_decode(&mut data, &work);
    pw_transpose(&mut data, 1, 1);
    pw_decode_bit_shuffle(&mut data, 1);
    pw_decode_bit_mix(&mut data);
    pw_rsa_decode(&mut data);
    pw_decode_bit_shuffle(&mut data, 0);
    pw_transpose(&mut data, 0, 0);
    pw_decode_substitution(&mut data);
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), data_out, 21);
    }
    1
}

/// Decoded password fields (`mMpswd_password_c`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PwFields {
    pub checksum: c_int,
    pub type_: c_int,
    pub hit_rate_index: c_int,
    pub npc_type: c_int,
    pub npc_code: c_int,
    pub item: u16,
    pub str0: [u8; 8],
    pub str1: [u8; 8],
}

/// `mMpswd_password` (`m_mail_password_check.c`): unpacks the
/// 21-byte payload into fields. For MAGAZINE the npc_type bit is
/// folded into hit_rate_index bit 2 and the npc fields become -1;
/// for other types the npc fields are -1 except POPULAR/CARD_E.
#[no_mangle]
pub extern "C" fn pc_pw_unpack(data: *const c_uchar, out: *mut PwFields) {
    if data.is_null() || out.is_null() {
        return;
    }
    let d = unsafe { std::slice::from_raw_parts(data, 21) };
    let b0 = d[0] as u32;
    let f = unsafe { &mut *out };
    f.checksum = ((b0 >> 3) & 3) as c_int;
    f.str0.copy_from_slice(&d[2..10]);
    f.str1.copy_from_slice(&d[10..18]);
    f.item = ((d[18] as u16) << 8) + d[19] as u16;
    f.type_ = ((b0 >> 5) & 7) as c_int;
    match f.type_ {
        t if t == PW_TYPE_POPULAR || t == PW_TYPE_CARD_E => {
            f.hit_rate_index = ((b0 >> 1) & 3) as c_int;
            f.npc_type = (b0 & 1) as c_int;
            f.npc_code = d[1] as c_int;
        }
        t if t == PW_TYPE_MAGAZINE => {
            f.hit_rate_index = ((((b0 >> 1) & 3) | (b0 & 1) << 2)) as c_int;
            f.npc_type = -1;
            f.npc_code = -1;
        }
        _ => {
            f.hit_rate_index = ((b0 >> 1) & 3) as c_int;
            f.npc_type = -1;
            f.npc_code = -1;
        }
    }
}

/// `mMpswd_password_zuru_check` (`m_mail_password_check.c`):
/// returns 1 when the payload is NOT cheated (type valid and the
/// recomputed two-bit checksum matches), 0 otherwise.
#[no_mangle]
pub extern "C" fn pc_pw_zuru_ok(fields: *const PwFields) -> c_int {
    if fields.is_null() {
        return 0;
    }
    let f = unsafe { &*fields };
    if f.type_ >= PW_TYPE_NUM {
        return 0;
    }
    let mut checksum: i32 = 0;
    for i in 0..8 {
        checksum += f.str0[i] as i32;
    }
    for i in 0..8 {
        checksum += f.str1[i] as i32;
    }
    checksum += f.item as i32;
    checksum += f.npc_code;
    ((checksum & 3) == f.checksum) as c_int
}

/// `mMpswd_check_pw_usable_letter_sub` (`m_editor_ovl.c`): returns 1
/// when `letter` is in the 64-entry allowed set, after normalizing
/// `0`->`O` and `1`->`l`. Space is not allowed.
#[no_mangle]
pub extern "C" fn pc_pw_usable_letter(letter: c_uchar) -> c_int {
    let mut l = letter;
    match l {
        48 => l = 79,  // CHAR_ZERO -> CHAR_O
        49 => l = 108, // CHAR_ONE -> CHAR_l
        _ => {}
    }
    PW_USABLE_LETTERS.contains(&l) as c_int
}

/// The make-overlay's town-nonempty gate (`mMl_strlen(town,
/// LAND_NAME_SIZE, CHAR_SPACE) > 0`): returns 1 when the town
/// name has at least one non-space byte.
#[no_mangle]
pub extern "C" fn pc_pw_make_can_advance(town: *const c_uchar) -> c_int {
    if town.is_null() {
        return 0;
    }
    let t = unsafe { std::slice::from_raw_parts(town, PW_TOWN_SIZE as usize) };
    let mut i = PW_TOWN_SIZE;
    while i != 0 {
        if t[(i - 1) as usize] != PW_CHAR_SPACE {
            return 1;
        }
        i -= 1;
    }
    0
}

/// `mED_input_pw_chk_line` (`m_editor_ovl.c`): fixed-width
/// character insertion into a 14-byte password line. `cursor` is
/// the in/out cursor index. Returns 0 when rejected (newline, or
/// the line's last byte is not a space), 1 when inserted, 2 when
/// inserted and the cursor reached the end of the line (the
/// caller switches from line0 to line1).
#[no_mangle]
pub extern "C" fn pc_pw_line_insert(line: *mut c_uchar, cursor: *mut c_int, code: c_uchar) -> c_int {
    if line.is_null() || cursor.is_null() {
        return 0;
    }
    if code == PW_CHAR_NEW_LINE {
        return 0;
    }
    let line = unsafe { std::slice::from_raw_parts_mut(line, PW_LINE_SIZE as usize) };
    if line[PW_LINE_SIZE as usize - 1] != PW_CHAR_SPACE {
        return 0;
    }
    let c = unsafe { *cursor };
    if c < PW_LINE_SIZE {
        for i in (c as usize..(PW_LINE_SIZE as usize - 1)).rev() {
            line[i + 1] = line[i];
        }
        line[c as usize] = code;
        unsafe {
            *cursor = c + 1;
        }
        if c + 1 >= PW_LINE_SIZE {
            2
        } else {
            1
        }
    } else {
        0
    }
}

/// `mED_backspace_func_pw_chk` (`m_editor_ovl.c`): password
/// backspace over the two 14-byte lines. `selected` is the
/// in/out selected line (0/1), `cursor` the in/out cursor index.
/// Within a line it moves left, shifts the tail, and space-fills
/// the last byte; at the start of line 1 it switches to line 0,
/// places the cursor at 13, and clears that byte.
#[no_mangle]
pub extern "C" fn pc_pw_backspace(
    line0: *mut c_uchar,
    line1: *mut c_uchar,
    selected: *mut c_int,
    cursor: *mut c_int,
) {
    if line0.is_null() || line1.is_null() || selected.is_null() || cursor.is_null() {
        return;
    }
    let (sel, cur) = unsafe { (*selected, *cursor) };
    let line = if sel == 1 {
        unsafe { std::slice::from_raw_parts_mut(line1, PW_LINE_SIZE as usize) }
    } else {
        unsafe { std::slice::from_raw_parts_mut(line0, PW_LINE_SIZE as usize) }
    };
    if cur != 0 {
        let c = (cur - 1) as usize;
        for i in c..(PW_LINE_SIZE as usize - 1) {
            line[i] = line[i + 1];
        }
        line[PW_LINE_SIZE as usize - 1] = PW_CHAR_SPACE;
        unsafe {
            *cursor = cur - 1;
        }
    } else if sel == 1 {
        let line0 = unsafe { std::slice::from_raw_parts_mut(line0, PW_LINE_SIZE as usize) };
        line0[PW_LINE_SIZE as usize - 1] = PW_CHAR_SPACE;
        unsafe {
            *selected = 0;
            *cursor = PW_LINE_SIZE - 1;
        }
    }
}

/// The `mED_edit_func_pw_chk` completion gate: the character-set
/// validation runs only when the last byte of line 1 is not a
/// space. Returns 1 when the charset check must run.
#[no_mangle]
pub extern "C" fn pc_pw_needs_charset_check(line1: *const c_uchar) -> c_int {
    if line1.is_null() {
        return 0;
    }
    (unsafe { *line1.add(PW_LINE_SIZE as usize - 1) } != PW_CHAR_SPACE) as c_int
}

/// `mED_check_pw_usable_letter` (`m_editor_ovl.c`): returns 1 when
/// all 28 bytes of both lines pass the allowed-character check.
#[no_mangle]
pub extern "C" fn pc_pw_all_usable(line0: *const c_uchar, line1: *const c_uchar) -> c_int {
    if line0.is_null() || line1.is_null() {
        return 0;
    }
    let (l0, l1) = unsafe {
        (
            std::slice::from_raw_parts(line0, PW_LINE_SIZE as usize),
            std::slice::from_raw_parts(line1, PW_LINE_SIZE as usize),
        )
    };
    (l0.iter().chain(l1.iter()).all(|&b| pc_pw_usable_letter(b) != 0)) as c_int
}

/// Make-overlay construction: fills the 8-byte town and player
/// buffers with `CHAR_SPACE`.
#[no_mangle]
pub extern "C" fn pc_pw_make_init(town: *mut c_uchar, player: *mut c_uchar) {
    if !town.is_null() {
        unsafe {
            std::ptr::write_bytes(town, PW_CHAR_SPACE, PW_TOWN_SIZE as usize);
        }
    }
    if !player.is_null() {
        unsafe {
            std::ptr::write_bytes(player, PW_CHAR_SPACE, PW_PLAYER_SIZE as usize);
        }
    }
}

/// Check-overlay construction nuance: line0/line1 are pre-filled
/// from the 28-byte `menu_info->data2`, not from spaces.
#[no_mangle]
pub extern "C" fn pc_pw_check_init(line0: *mut c_uchar, line1: *mut c_uchar, src28: *const c_uchar) {
    if line0.is_null() || line1.is_null() || src28.is_null() {
        return;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(src28, line0, PW_LINE_SIZE as usize);
        std::ptr::copy_nonoverlapping(src28.add(PW_LINE_SIZE as usize), line1, PW_LINE_SIZE as usize);
    }
}

/// Mail-body code extraction (`mMpswd_restore_code` =
/// `mMpswd_check_opening_sentence` +
/// `mMpswd_get_password_pointer` + `mMpswd_except_return_code`,
/// `m_mail_password_check.c`): requires the `CHAR_SYMBOL_KEY`
/// opening mark, scans up to 32 bytes for a newline followed 15
/// bytes later by another newline, then copies 28 bytes skipping
/// the byte-14 newline. Returns 1 and fills `out28` on success,
/// 0 otherwise.
#[no_mangle]
pub extern "C" fn pc_pw_extract_code(body: *const c_uchar, body_len: c_int, out28: *mut c_uchar) -> c_int {
    if body.is_null() || out28.is_null() || body_len < 29 {
        return 0;
    }
    let body = unsafe { std::slice::from_raw_parts(body, body_len as usize) };
    if body[0] != PW_CHAR_SYMBOL_KEY {
        return 0;
    }
    let mut ptr: Option<usize> = None;
    for i in 0..32.min(body.len()) {
        if body[i] == PW_CHAR_NEW_LINE {
            if i + 15 < body.len() && body[i + 15] == PW_CHAR_NEW_LINE {
                ptr = Some(i + 1);
            }
            break;
        }
    }
    let p = match ptr {
        Some(p) => p,
        None => return 0,
    };
    if p + 29 > body.len() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out28, 28) };
    let mut di = 0;
    for i in 0..29 {
        // src[14] is always going to be the newline character
        if i != 14 {
            out[di] = body[p + i];
            di += 1;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &[u8]) -> [u8; 8] {
        let mut n = [PW_CHAR_SPACE; 8];
        n[..s.len().min(8)].copy_from_slice(&s[..s.len().min(8)]);
        n
    }

    #[test]
    fn codec_round_trip() {
        let s0 = name(b"PHILIP");
        let s1 = name(b"SHMANIMA");
        for (type_, hit, npc_t, npc_c) in [
            (PW_TYPE_USER, 1, 0, 0xFF),
            (PW_TYPE_FAMICOM, 1, 0, 0xFF),
            (PW_TYPE_POPULAR, 4, 1, 3),
            (PW_TYPE_CARD_E, 2, 0, 10),
            (PW_TYPE_MAGAZINE, 5, 0, 0xFF),
            (PW_TYPE_CARD_E_MINI, 1, 0, 0xFF),
        ] {
            let mut code = [0u8; 28];
            pc_pw_make_password(type_, hit, s0.as_ptr(), s1.as_ptr(), 0x1234, npc_t, npc_c, code.as_mut_ptr());
            // every emitted char is in the alphabet
            for &c in &code {
                assert!(PW_FONTNUM.contains(&c), "type {type_} emitted {c:#x}");
            }
            let mut data = [0u8; 21];
            assert_eq!(pc_pw_decode_code(data.as_mut_ptr(), code.as_ptr()), 1);
            let mut f = PwFields::default();
            pc_pw_unpack(data.as_ptr(), &mut f as *mut PwFields);
            assert_eq!(pc_pw_zuru_ok(&f as *const PwFields), 1, "type {type_}");
            assert_eq!(f.str0, s0);
            assert_eq!(f.str1, s1);
            assert_eq!(f.item, 0x1234);
        }
    }

    #[test]
    fn decode_rejects_bad_chars() {
        let mut code = [b'!'; 28]; // '!' not in the alphabet
        let mut data = [0u8; 21];
        assert_eq!(pc_pw_decode_code(data.as_mut_ptr(), code.as_ptr()), 0);
        // zero/one normalization: '0' -> 'O' is in the alphabet
        code = [b'0'; 28];
        assert_eq!(pc_pw_decode_code(data.as_mut_ptr(), code.as_ptr()), 1);
    }

    #[test]
    fn unpack_magazine_bit_fold() {
        // MAGAZINE: hit_rate 5 -> npc_type bit folded into hit_rate_index bit 2
        let s0 = name(b"A");
        let s1 = name(b"B");
        let mut code = [0u8; 28];
        pc_pw_make_password(PW_TYPE_MAGAZINE, 5, s0.as_ptr(), s1.as_ptr(), 1, 0, 0xFF, code.as_mut_ptr());
        let mut data = [0u8; 21];
        assert_eq!(pc_pw_decode_code(data.as_mut_ptr(), code.as_ptr()), 1);
        let mut f = PwFields::default();
        pc_pw_unpack(data.as_ptr(), &mut f as *mut PwFields);
        assert_eq!(f.type_, PW_TYPE_MAGAZINE);
        assert_eq!(f.hit_rate_index, 5);
        assert_eq!(f.npc_type, -1);
        assert_eq!(f.npc_code, -1);
    }

    #[test]
    fn popular_hit_rate_quirk() {
        // POPULAR forces hit_rate = 4, which collides with checksum bits
        let s0 = name(b"A");
        let s1 = name(b"B");
        let mut code = [0u8; 28];
        pc_pw_make_password(PW_TYPE_POPULAR, 0, s0.as_ptr(), s1.as_ptr(), 1, 1, 7, code.as_mut_ptr());
        let mut data = [0u8; 21];
        assert_eq!(pc_pw_decode_code(data.as_mut_ptr(), code.as_ptr()), 1);
        let mut f = PwFields::default();
        pc_pw_unpack(data.as_ptr(), &mut f as *mut PwFields);
        assert_eq!(f.type_, PW_TYPE_POPULAR);
        assert_eq!(f.npc_code, 7);
        assert_eq!(pc_pw_zuru_ok(&f as *const PwFields), 1);
    }

    #[test]
    fn usable_letters() {
        assert_eq!(pc_pw_usable_letter(b'A'), 1);
        assert_eq!(pc_pw_usable_letter(b'z'), 1);
        assert_eq!(pc_pw_usable_letter(b'5'), 1);
        assert_eq!(pc_pw_usable_letter(b'%'), 1);
        assert_eq!(pc_pw_usable_letter(0xD1), 1);
        assert_eq!(pc_pw_usable_letter(b'@'), 1);
        assert_eq!(pc_pw_usable_letter(b'0'), 1); // normalized to O
        assert_eq!(pc_pw_usable_letter(b'1'), 1); // normalized to l
        assert_eq!(pc_pw_usable_letter(b' '), 0);
        assert_eq!(pc_pw_usable_letter(b'!'), 0);
    }

    #[test]
    fn line_insert_machine() {
        let mut line = [PW_CHAR_SPACE; 14];
        let mut cursor = 0;
        assert_eq!(pc_pw_line_insert(line.as_mut_ptr(), &mut cursor, b'A'), 1);
        assert_eq!(pc_pw_line_insert(line.as_mut_ptr(), &mut cursor, b'B'), 1);
        assert_eq!(line[0], b'A');
        assert_eq!(line[1], b'B');
        assert_eq!(cursor, 2);
        // insert in the middle shifts right
        cursor = 1;
        assert_eq!(pc_pw_line_insert(line.as_mut_ptr(), &mut cursor, b'X'), 1);
        assert_eq!(&line[..4], &[b'A', b'X', b'B', PW_CHAR_SPACE]);
        // newline rejected
        assert_eq!(pc_pw_line_insert(line.as_mut_ptr(), &mut cursor, PW_CHAR_NEW_LINE), 0);
        // fill to the end -> returns 2
        cursor = 13;
        assert_eq!(pc_pw_line_insert(line.as_mut_ptr(), &mut cursor, b'Z'), 2);
        assert_eq!(cursor, 14);
        // full line rejects
        assert_eq!(pc_pw_line_insert(line.as_mut_ptr(), &mut cursor, b'Q'), 0);
    }

    #[test]
    fn backspace_machine() {
        let mut l0 = *b"AB            ";
        let mut l1 = *b"CD            ";
        let (mut sel, mut cur) = (1, 2);
        pc_pw_backspace(l0.as_mut_ptr(), l1.as_mut_ptr(), &mut sel, &mut cur);
        assert_eq!(&l1[..3], b"C  ");
        assert_eq!((sel, cur), (1, 1));
        // at start of line 1 -> switch to line 0, clear its last byte
        cur = 0;
        pc_pw_backspace(l0.as_mut_ptr(), l1.as_mut_ptr(), &mut sel, &mut cur);
        assert_eq!((sel, cur), (0, 13));
        assert_eq!(l0[13], PW_CHAR_SPACE);
    }

    #[test]
    fn completion_gate() {
        let full = [b'A'; 14];
        let partial = [PW_CHAR_SPACE; 14];
        assert_eq!(pc_pw_needs_charset_check(full.as_ptr()), 1);
        assert_eq!(pc_pw_needs_charset_check(partial.as_ptr()), 0);
        let l0 = [b'A'; 14];
        assert_eq!(pc_pw_all_usable(l0.as_ptr(), full.as_ptr()), 1);
        assert_eq!(pc_pw_all_usable(l0.as_ptr(), partial.as_ptr()), 0); // spaces not allowed
    }

    #[test]
    fn make_flow_helpers() {
        let mut town = [PW_CHAR_SPACE; 8];
        let mut player = [PW_CHAR_SPACE; 8];
        pc_pw_make_init(town.as_mut_ptr(), player.as_mut_ptr());
        assert_eq!(pc_pw_make_can_advance(town.as_ptr()), 0);
        town[0] = b'X';
        assert_eq!(pc_pw_make_can_advance(town.as_ptr()), 1);
        // check-init prefills from data2
        let src = [b'Q'; 28];
        let (mut l0, mut l1) = ([0u8; 14], [0u8; 14]);
        pc_pw_check_init(l0.as_mut_ptr(), l1.as_mut_ptr(), src.as_ptr());
        assert_eq!(l0, [b'Q'; 14]);
        assert_eq!(l1, [b'Q'; 14]);
    }

    #[test]
    fn extract_code() {
        // body: SYMBOL_KEY, newline, 14 chars, newline, 14 chars
        let mut body = [PW_CHAR_SPACE; 64];
        body[0] = PW_CHAR_SYMBOL_KEY;
        body[1] = PW_CHAR_NEW_LINE;
        for i in 0..14 {
            body[2 + i] = b'A' + (i as u8 % 26);
        }
        body[16] = PW_CHAR_NEW_LINE;
        for i in 0..14 {
            body[17 + i] = b'a' + (i as u8 % 26);
        }
        let mut out = [0u8; 28];
        assert_eq!(pc_pw_extract_code(body.as_ptr(), 64, out.as_mut_ptr()), 1);
        assert_eq!(out.len(), 28);
        assert!(!out.contains(&PW_CHAR_NEW_LINE));
        assert_eq!(&out[..14], &[b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L', b'M', b'N']);
        // missing opening mark -> 0
        body[0] = b'X';
        assert_eq!(pc_pw_extract_code(body.as_ptr(), 64, out.as_mut_ptr()), 0);
    }
}
