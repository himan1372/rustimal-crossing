//! Rust port of the bounded repeated-character check used for villager mail.
//!
//! The game-facing `mNpc_CheckNormalMail_sub` C ABI is retained. The source
//! algorithm scans the fixed 192-byte mail body and applies different repeat
//! limits to ordinary characters and punctuation/control symbols.

const MAIL_BODY_LEN: usize = 192;
const CHAR_SPACE: u8 = 32;
const CHAR_EXCLAMATION: u8 = 33;
const CHAR_QUOTATION: u8 = 34;
const CHAR_PERCENT: u8 = 37;
const CHAR_AT_SIGN: u8 = 64;
const CHAR_SYMBOL_ANNOYED: u8 = 92;
const CHAR_UNDERSCORE: u8 = 95;
const CHAR_CONTROL_CODE: u8 = 127;
const CHAR_INTERPUNCT: u8 = 133;
const CHAR_HYPHEN: u8 = 144;

fn is_mail_symbol(character: u8) -> bool {
    character == CHAR_EXCLAMATION
        || character == CHAR_QUOTATION
        || character == CHAR_UNDERSCORE
        || character == CHAR_HYPHEN
        || character == CHAR_SYMBOL_ANNOYED
        || (CHAR_PERCENT..=CHAR_AT_SIGN).contains(&character)
        || (CHAR_CONTROL_CODE..=CHAR_INTERPUNCT).contains(&character)
}

pub(crate) fn check_normal_mail(body: &[u8; MAIL_BODY_LEN]) -> (i32, i32) {
    let mut last_character = CHAR_SPACE;
    let mut run_length = 1;
    let mut character_count = 0;
    let mut consecutive_characters = false;
    let mut scanned = 0;

    for &character in body {
        if character != CHAR_SPACE {
            if last_character == character {
                run_length += 1;
                if run_length >= 3 {
                    if is_mail_symbol(character) {
                        if run_length >= 8 {
                            consecutive_characters = true;
                            break;
                        }
                    } else {
                        consecutive_characters = true;
                        break;
                    }
                }
            } else {
                run_length = 0;
                last_character = character;
            }

            character_count += 1;
        }
        scanned += 1;
    }

    // The C routine breaks before advancing its pointer, then counts from the
    // current byte here. This also counts the byte that triggered the limit.
    for &character in &body[scanned..] {
        if character != CHAR_SPACE {
            character_count += 1;
        }
    }

    (character_count, if consecutive_characters { 1 } else { 0 })
}

/// Checks the fixed-size letter body for repeated characters.
///
/// # Safety
/// `char_num` must be writable and `body` must point to at least 192 readable
/// bytes, matching `MAIL_BODY_LEN` in `include/m_mail.h`.
#[no_mangle]
pub unsafe extern "C" fn mNpc_CheckNormalMail_sub(char_num: *mut i32, body: *const u8) -> i32 {
    if char_num.is_null() || body.is_null() {
        return 0;
    }

    // SAFETY: The caller supplies a fixed-size mail body per the existing C ABI.
    let body = unsafe { std::slice::from_raw_parts(body, MAIL_BODY_LEN) };
    let Ok(body) = <&[u8; MAIL_BODY_LEN]>::try_from(body) else {
        return 0;
    };
    let (count, consecutive) = check_normal_mail(body);

    // SAFETY: The pointer was checked above and is writable by the ABI contract.
    unsafe { *char_num = count };
    consecutive
}

#[cfg(test)]
mod tests {
    use super::{
        check_normal_mail, is_mail_symbol, mNpc_CheckNormalMail_sub, CHAR_SPACE, MAIL_BODY_LEN,
    };

    fn body_with(prefix: &[u8]) -> [u8; MAIL_BODY_LEN] {
        let mut body = [CHAR_SPACE; MAIL_BODY_LEN];
        body[..prefix.len()].copy_from_slice(prefix);
        body
    }

    #[test]
    fn counts_non_space_characters_in_a_padded_body() {
        assert_eq!(check_normal_mail(&body_with(b"Hi!")), (3, 0));
    }

    #[test]
    fn ordinary_character_limit_matches_the_c_loop() {
        assert_eq!(check_normal_mail(&body_with(b"aaaa")), (4, 1));
        assert_eq!(check_normal_mail(&body_with(b"aaa")), (3, 0));
    }

    #[test]
    fn punctuation_uses_the_higher_repeat_limit() {
        let symbol = [92; 8];
        assert_eq!(check_normal_mail(&body_with(&symbol)), (8, 0));
        let symbol = [92; 9];
        assert_eq!(check_normal_mail(&body_with(&symbol)), (9, 1));
    }

    #[test]
    fn spaces_do_not_add_to_the_reported_character_count() {
        assert_eq!(check_normal_mail(&body_with(b"a a a")), (3, 0));
        assert_eq!(check_normal_mail(&body_with(b"a a a a")), (4, 1));
    }
    #[test]
    fn symbol_classification_boundaries() {
        // %..=@ are symbols; the neighbours are not.
        assert!(is_mail_symbol(37)); // '%'
        assert!(is_mail_symbol(64)); // '@'
        assert!(!is_mail_symbol(36));
        assert!(!is_mail_symbol(65)); // 'A'
        // 127..=133 are symbols; 134 is not.
        assert!(is_mail_symbol(127));
        assert!(is_mail_symbol(133));
        assert!(!is_mail_symbol(134));
        // Named singletons.
        assert!(is_mail_symbol(33)); // '!'
        assert!(is_mail_symbol(34)); // '"'
        assert!(is_mail_symbol(95)); // '_'
        assert!(is_mail_symbol(144)); // hyphen
        assert!(!is_mail_symbol(145));
        assert!(!is_mail_symbol(b'a'));
        assert!(!is_mail_symbol(CHAR_SPACE));
    }

    #[test]
    fn symbol_repeat_needs_eight() {
        // Seven backslashes: run too short to flag.
        assert_eq!(check_normal_mail(&body_with(&[92; 7])), (7, 0));
        // Mixed runs each under the limit are fine.
        assert_eq!(check_normal_mail(&body_with(b"aa!!bb")), (6, 0));
        // An ordinary char hitting exactly 3 flags; a symbol at 7 does not.
        assert_eq!(check_normal_mail(&body_with(b"xyz!!!qqqq")), (10, 1));
        // C ABI null-safety.
        unsafe {
            assert_eq!(mNpc_CheckNormalMail_sub(std::ptr::null_mut(), std::ptr::null()), 0);
            let body = body_with(b"Hi!");
            let mut count = -1i32;
            let flagged = mNpc_CheckNormalMail_sub(&mut count, body.as_ptr());
            assert_eq!((count, flagged), (3, 0));
        }
    }

}
