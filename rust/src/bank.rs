//! ABD bank overlay amount-editing arithmetic — pure kernels.
//!
//! Verified against `src/game/m_bank_ovl.c`, `include/m_bank_ovl.h`,
//! `include/m_private.h`, `src/game/m_mail.c`,
//! `src/actor/npc/ac_npc_post_girl.c`,
//! `src/actor/npc/ac_npc_post_girl.c_inc`, `src/save_check_NSW.c_inc`
//! (GAFE01_00 Rev 0 USA decomp). All 13 brief claims checked; no
//! behavioral corrections. Two brief imprecisions carried here: the
//! smallest place value is 1, not 10; `player_max_bell` has the exact
//! formula `99999 + 30000 × (money bags of any denomination) +
//! 30000 × (empty slots)`, valuing every existing bag at the largest
//! bag size.
//!
//! Deliberately NOT ported: `mBN_bank_ovl_init` (reads the wallet and
//! inventory), `mBN_bank_ok` (mutates inventory, wallet, and the save),
//! the submenu input state machine, Pelly's dialogue, and the mail
//! send. The UI edits the proposed cash and bank amounts together in
//! memory; the commit reconciles the wallet and physical Bell-bag
//! items. Do not collapse deposit/withdrawal into "assign two
//! numbers": the original can remove, upgrade, or create Bell bags to
//! represent the proposed total.
//!
//! Also established: the overlay computes NO interest — `bank_account`
//! is referenced only in the overlay, the milestone mail, Pelly's
//! balance message, and save validation.

/// Bank overlay limits (`include/m_bank_ovl.h`, `include/m_private.h`).
pub mod limits {
    /// Maximum account balance (both `mBN_DEPOSIT_MAX` and
    /// `mPr_DEPOSIT_MAX`, defined separately, same value).
    pub const DEPOSIT_MAX: i32 = 999_999_999;
    /// Maximum wallet cash (`mPr_WALLET_MAX`).
    pub const WALLET_MAX: i32 = 99_999;
    /// Largest cursor digit index (`mBN_CURSOL_MAX`).
    pub const CURSOL_MAX: u8 = 5;
    /// Cursor index of the OK option (`mBN_CUSROL_OK`).
    pub const CURSOL_OK: u8 = 6;
    /// Bell-bag values (`aNSM_sack_amount`): 100 / 1,000 / 10,000 /
    /// 30,000, for ITM_MONEY_100 .. ITM_MONEY_30000. All queries use
    /// `mPr_ITEM_COND_NORMAL` — an item-ID match alone is not enough.
    pub const SACK_AMOUNTS: [i32; 4] = [100, 1_000, 10_000, 30_000];
}

/// Cursor-to-place-value (`mBN_cursol_2_keta`), verbatim:
/// `keta = 1; while (cursol++ < 5) keta *= 10;`
/// Mapping: 0→100000, 1→10000, 2→1000, 3→100, 4→10, 5→1; cursor 6 is
/// the OK option and carries no amount (retail never passes it).
pub fn cursol_2_keta(cursol: u8) -> u32 {
    let mut keta: u32 = 1;
    let mut c = cursol;
    while c < limits::CURSOL_MAX {
        keta *= 10;
        c += 1;
    }
    keta
}

/// Displayed cash change (`mBN_now_bell_2_bell`), verbatim:
/// `bell = ABS(now_bell - player_bell)`.
pub fn bell_change(now_bell: i32, player_bell: i32) -> i32 {
    (now_bell - player_bell).abs()
}

/// The bank overlay's working values (`struct bank_ovl_s`). The saved
/// `bank_account` field is u32; the overlay works in signed int —
/// do not change the save field's type to match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BankOvl {
    pub player_max_bell: i32,
    pub player_bell: i32,
    pub now_bell: i32,
    pub bank_bell: i32,
    pub cursol: i32,
    pub bell: i32,
}

/// Outcome of an edit attempt. `Buzz` means the increment computed to
/// zero: the buzzer (`MONO(3)` = 0x1003) plays and the function
/// returns WITHOUT updating `bell` or playing the success sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    Ok,
    Buzz,
}

/// Deposit branch (C-up on a digit), verbatim. Order is load-bearing:
/// cash-availability clamp → bank-max clamp → zero check (buzzer,
/// return before the crossing check) → crossing check. The crossing
/// check only fires when a withdrawal already pushed `now_bell` above
/// the opening amount: a deposit is then capped so `now_bell` lands
/// exactly on `player_bell` instead of crossing below it.
pub fn deposit(state: &mut BankOvl, mut keta: i32) -> EditOutcome {
    if state.now_bell < keta {
        keta = state.now_bell;
    }
    if keta + state.bank_bell > limits::DEPOSIT_MAX {
        keta = limits::DEPOSIT_MAX - state.bank_bell;
    }
    if keta == 0 {
        return EditOutcome::Buzz;
    }
    if state.now_bell > state.player_bell
        && (state.now_bell - keta) < state.player_bell
    {
        keta = state.now_bell - state.player_bell;
    }
    state.bank_bell += keta;
    state.now_bell -= keta;
    state.bell = bell_change(state.now_bell, state.player_bell);
    EditOutcome::Ok
}

/// Withdrawal branch (C-down on a digit), verbatim. Same
/// clamp→clamp→zero→crossing order; the crossing check mirrors the
/// deposit side (caps so `now_bell` lands exactly on `player_bell`
/// when coming from below).
pub fn withdraw(state: &mut BankOvl, mut keta: i32) -> EditOutcome {
    if state.bank_bell < keta {
        keta = state.bank_bell;
    }
    if state.now_bell + keta > state.player_max_bell {
        keta = state.player_max_bell - state.now_bell;
    }
    if keta == 0 {
        return EditOutcome::Buzz;
    }
    if state.now_bell < state.player_bell
        && (state.now_bell + keta) > state.player_bell
    {
        keta = state.player_bell - state.now_bell;
    }
    state.bank_bell -= keta;
    state.now_bell += keta;
    state.bell = bell_change(state.now_bell, state.player_bell);
    EditOutcome::Ok
}

/// Pelly's bank-account eligibility (`aPG_actor_ct`), verbatim: native
/// player, loan fully repaid, house size ≥ 3, not mid-renewal. Set once
/// at actor construction into `postgirl->has_bank_account`.
pub fn has_bank_account(is_native: bool, loan: u32, house_size: u8, renew: bool) -> bool {
    is_native && loan == 0 && house_size >= 3 && !renew
}

/// Balance milestone gifts (`l_mml_postoffice_info`, `m_mail.c`).
/// Checked ascending; the first tier whose balance is met AND whose
/// per-player flag is unset is sent (the flag is set only if the mail
/// send succeeds); the loop then `break`s. Gift items are wrapped in
/// `FTR_START()` in C; flag values live in `m_private.h` and are
/// passed in here by the caller (in C).
pub mod milestone {
    /// (required balance, mail no) per tier, ascending.
    pub const TIERS: [(u32, u16); 4] = [
        (1_000_000, 0x0246),
        (10_000_000, 0x0247),
        (100_000_000, 0x0248),
        (999_999_999, 0x0249),
    ];

    /// Returns the tier index (0..3) whose gift should be sent, or
    /// `None` when no tier is newly eligible. `claimed` is the
    /// per-player received-flags word; bit `i` corresponds to tier `i`.
    pub fn eligible_tier(balance: u32, claimed: u32) -> Option<usize> {
        for (i, &(required, _)) in TIERS.iter().enumerate() {
            if balance >= required && (claimed & (1 << i)) == 0 {
                return Some(i);
            }
        }
        None
    }
}

/// Save-data validation (`sCck_CheckSaveData_deposit_NSW`), verbatim:
/// the u32 field is read into a signed int and checked literally as
/// `deposit < 0 || deposit > mPr_DEPOSIT_MAX`. A balance ≥ 2^31 would
/// fail; retail can never produce one (the overlay clamps to
/// 999,999,999). Preserve the signed comparison literally.
pub fn deposit_save_error(deposit: i32) -> bool {
    deposit < 0 || deposit > limits::DEPOSIT_MAX
}

// ---- C ABI ----
// All pc_-prefixed: the C originals stay compiled; these are staged kernels.

/// C-visible mirror of [`BankOvl`].
#[repr(C)]
pub struct BankOvlC {
    pub player_max_bell: i32,
    pub player_bell: i32,
    pub now_bell: i32,
    pub bank_bell: i32,
    pub cursol: i32,
    pub bell: i32,
}

fn from_c(s: &BankOvlC) -> BankOvl {
    BankOvl {
        player_max_bell: s.player_max_bell,
        player_bell: s.player_bell,
        now_bell: s.now_bell,
        bank_bell: s.bank_bell,
        cursol: s.cursol,
        bell: s.bell,
    }
}

fn to_c(s: &BankOvl, out: &mut BankOvlC) {
    out.player_max_bell = s.player_max_bell;
    out.player_bell = s.player_bell;
    out.now_bell = s.now_bell;
    out.bank_bell = s.bank_bell;
    out.cursol = s.cursol;
    out.bell = s.bell;
}

/// Returns 1 when the edit applied, 0 when it buzzed (no change).
#[no_mangle]
pub extern "C" fn pc_bn_deposit(state: *mut BankOvlC, keta: i32) -> u8 {
    if state.is_null() {
        return 0;
    }
    let s = unsafe { &mut *state };
    let mut st = from_c(s);
    let ok = deposit(&mut st, keta) == EditOutcome::Ok;
    to_c(&st, s);
    ok as u8
}

/// Returns 1 when the edit applied, 0 when it buzzed (no change).
#[no_mangle]
pub extern "C" fn pc_bn_withdraw(state: *mut BankOvlC, keta: i32) -> u8 {
    if state.is_null() {
        return 0;
    }
    let s = unsafe { &mut *state };
    let mut st = from_c(s);
    let ok = withdraw(&mut st, keta) == EditOutcome::Ok;
    to_c(&st, s);
    ok as u8
}

#[no_mangle]
pub extern "C" fn pc_bn_keta(cursol: u8) -> u32 {
    cursol_2_keta(cursol)
}

#[no_mangle]
pub extern "C" fn pc_bn_bell_change(now_bell: i32, player_bell: i32) -> i32 {
    bell_change(now_bell, player_bell)
}

#[no_mangle]
pub extern "C" fn pc_bn_has_bank_account(
    is_native: u8,
    loan: u32,
    house_size: u8,
    renew: u8,
) -> u8 {
    has_bank_account(is_native != 0, loan, house_size, renew != 0) as u8
}

/// Tier index 0..3 of the newly-eligible milestone gift, or -1.
#[no_mangle]
pub extern "C" fn pc_bn_milestone(balance: u32, claimed: u32) -> i8 {
    match milestone::eligible_tier(balance, claimed) {
        Some(i) => i as i8,
        None => -1,
    }
}

#[no_mangle]
pub extern "C" fn pc_bn_deposit_save_error(deposit: i32) -> u8 {
    deposit_save_error(deposit) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> BankOvl {
        BankOvl {
            player_max_bell: limits::WALLET_MAX + 30_000,
            player_bell: 50_000,
            now_bell: 50_000,
            bank_bell: 100_000,
            cursol: 0,
            bell: 0,
        }
    }

    #[test]
    fn keta_table() {
        // The brief's "smallest is 10" hint was wrong: it is 1.
        assert_eq!(cursol_2_keta(0), 100_000);
        assert_eq!(cursol_2_keta(1), 10_000);
        assert_eq!(cursol_2_keta(2), 1_000);
        assert_eq!(cursol_2_keta(3), 100);
        assert_eq!(cursol_2_keta(4), 10);
        assert_eq!(cursol_2_keta(5), 1);
        assert_eq!(limits::CURSOL_OK, 6);
        assert_eq!(limits::DEPOSIT_MAX, 999_999_999);
        assert_eq!(limits::WALLET_MAX, 99_999);
        assert_eq!(limits::SACK_AMOUNTS, [100, 1_000, 10_000, 30_000]);
    }

    #[test]
    fn bell_change_abs() {
        assert_eq!(bell_change(50_000, 50_000), 0);
        assert_eq!(bell_change(40_000, 50_000), 10_000);
        assert_eq!(bell_change(60_000, 50_000), 10_000);
    }

    #[test]
    fn deposit_basic() {
        let mut s = fresh();
        assert_eq!(deposit(&mut s, 1_000), EditOutcome::Ok);
        assert_eq!(s.bank_bell, 101_000);
        assert_eq!(s.now_bell, 49_000);
        assert_eq!(s.bell, 1_000);
    }

    #[test]
    fn deposit_clamped_to_cash() {
        let mut s = fresh();
        assert_eq!(deposit(&mut s, 1_000_000), EditOutcome::Ok);
        assert_eq!(s.bank_bell, 150_000);
        assert_eq!(s.now_bell, 0);
        assert_eq!(s.bell, 50_000);
    }

    #[test]
    fn deposit_clamped_to_max() {
        let mut s = fresh();
        s.bank_bell = limits::DEPOSIT_MAX - 500;
        s.now_bell = 50_000;
        assert_eq!(deposit(&mut s, 1_000), EditOutcome::Ok);
        assert_eq!(s.bank_bell, limits::DEPOSIT_MAX);
        assert_eq!(s.now_bell, 49_500);
    }

    #[test]
    fn deposit_buzzes_at_zero() {
        let mut s = fresh();
        s.now_bell = 0;
        s.bank_bell = 100_000;
        let before = s;
        // zero-increment buzzes and leaves everything untouched
        assert_eq!(deposit(&mut s, 1_000), EditOutcome::Buzz);
        assert_eq!(s, before);
    }

    #[test]
    fn deposit_crossing_cap() {
        // Withdraw first so now_bell > player_bell, then a deposit
        // that would cross below player_bell is capped to land on it.
        let mut s = fresh();
        assert_eq!(withdraw(&mut s, 30_000), EditOutcome::Ok);
        assert_eq!(s.now_bell, 80_000);
        assert_eq!(deposit(&mut s, 50_000), EditOutcome::Ok);
        assert_eq!(s.now_bell, 50_000); // lands exactly on player_bell
        assert_eq!(s.bank_bell, 100_000); // net 30k moved back
        assert_eq!(s.bell, 0);
    }

    #[test]
    fn withdraw_basic_and_capacity() {
        let mut s = fresh();
        assert_eq!(withdraw(&mut s, 10_000), EditOutcome::Ok);
        assert_eq!(s.bank_bell, 90_000);
        assert_eq!(s.now_bell, 60_000);
        // capped by player_max_bell
        s.player_max_bell = 65_000;
        assert_eq!(withdraw(&mut s, 10_000), EditOutcome::Ok);
        assert_eq!(s.now_bell, 65_000);
        assert_eq!(s.bank_bell, 85_000);
        // capped by the bank balance itself
        s.bank_bell = 2_000;
        assert_eq!(withdraw(&mut s, 10_000), EditOutcome::Ok);
        assert_eq!(s.bank_bell, 0);
        assert_eq!(s.now_bell, 65_000); // capacity cap still binds
        // empty bank buzzes
        let before = s;
        assert_eq!(withdraw(&mut s, 1), EditOutcome::Buzz);
        assert_eq!(s, before);
    }

    #[test]
    fn withdraw_crossing_cap() {
        // Deposit first so now_bell < player_bell, then a withdrawal
        // that would cross above player_bell is capped to land on it.
        let mut s = fresh();
        assert_eq!(deposit(&mut s, 30_000), EditOutcome::Ok);
        assert_eq!(s.now_bell, 20_000);
        assert_eq!(withdraw(&mut s, 50_000), EditOutcome::Ok);
        assert_eq!(s.now_bell, 50_000);
        assert_eq!(s.bank_bell, 100_000);
        assert_eq!(s.bell, 0);
    }

    #[test]
    fn eligibility() {
        assert!(has_bank_account(true, 0, 3, false));
        assert!(!has_bank_account(false, 0, 3, false));
        assert!(!has_bank_account(true, 1, 3, false));
        assert!(!has_bank_account(true, 0, 2, false));
        assert!(!has_bank_account(true, 0, 3, true));
        assert!(has_bank_account(true, 0, 8, false));
    }

    #[test]
    fn milestones() {
        use milestone::*;
        assert_eq!(eligible_tier(0, 0), None);
        assert_eq!(eligible_tier(999_999, 0), None);
        assert_eq!(eligible_tier(1_000_000, 0), Some(0));
        assert_eq!(eligible_tier(50_000_000, 0), Some(0)); // ascending: first tier first
        assert_eq!(eligible_tier(50_000_000, 0b0001), Some(1));
        assert_eq!(eligible_tier(999_999_999, 0b0111), Some(3));
        assert_eq!(eligible_tier(999_999_999, 0b1111), None);
        assert_eq!(TIERS[3], (999_999_999, 0x0249));
    }

    #[test]
    fn save_validation() {
        assert!(!deposit_save_error(0));
        assert!(!deposit_save_error(999_999_999));
        assert!(deposit_save_error(1_000_000_000));
        assert!(deposit_save_error(-1));
    }
}
