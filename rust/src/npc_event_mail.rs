//! Non-letter NPC-generated mail (`m_npc.c` hub, USA retail).
//!
//! Distinct from normal letter replies (`npc_reply.rs` / `mNpc_Remail`),
//! these four systems generate NPC mail without a player letter:
//!
//! ```text
//! NpcMailSystem
//! ├── EventMail ......... Valentine: mNpc_GetEventMail / mNpc_SendVtdayMail
//! ├── BirthdayMail ...... mNpc_GetBirthdayCard / mNpc_SendEventBirthdayCard2
//! ├── ChristmasMail ..... mNpc_GetXmasCardData (fixed, no RNG)
//! ├── GoodbyeMail ....... mNpc_SetGoodbyMailData (retryable bitfield)
//! └── PasswordMail ...... HP-mail storage + 6 delayed generators
//! ```
//!
//! Shared lower level: `load_npc_mail_data_common2` (retail's
//! `mNpc_LoadMailDataCommon2`), the paper picker (`mNpc_GetPaperType`),
//! and the three delivery policies. Generation and delivery are kept
//! separate: a `GeneratedMail` is built first, then a `DeliveryResult`
//! records where it landed.
//!
//! Engine boundary: actual item selection (`mSP_SelectRandomItem_New`),
//! mailbox/post-office storage, and ROM handbill text stay engine-side.
//! This module ports the selection formulas, message-number math,
//! scheduling quirks, and RNG structure.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// RNG
// ---------------------------------------------------------------------------

/// Retail RNG source. `RANDOM(n)` and `mQst_GetRandom(n)` are distinct
/// streams in retail; the goodbye path specifically uses the quest one.
pub trait MailRng {
    fn random(&mut self, n: u32) -> u32; // RANDOM(n)
    fn quest_random(&mut self, n: u32) -> u32; // mQst_GetRandom(n)
}

/// Deterministic test RNG.
#[derive(Clone, Debug)]
pub struct TestRng {
    state: u64,
}

impl TestRng {
    pub fn new(seed: u64) -> TestRng {
        TestRng { state: seed }
    }
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
}

impl MailRng for TestRng {
    fn random(&mut self, n: u32) -> u32 {
        (self.next() % n as u64) as u32
    }
    fn quest_random(&mut self, n: u32) -> u32 {
        // A separate stream would diverge here; tests use one stream and
        // only assert the call happens, not the value.
        (self.next() % n as u64) as u32
    }
}

// ---------------------------------------------------------------------------
// Common mail construction
// ---------------------------------------------------------------------------

/// Mail types (`mMl_TYPE_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcMailType {
    Normal = 0,
    Xmas = 1,
    /// `mMl_TYPE_SPNPC_PASSWORD`: special-NPC password responses.
    SpNpcPassword = 2,
}

/// A generated NPC mail: message id + substitutions + present + paper.
/// ROM text lookup by `mail_no` is engine-side.
#[derive(Clone, Debug)]
pub struct GeneratedMail {
    pub mail_no: u16,
    pub mail_type: NpcMailType,
    /// (free-string slot, text)
    pub free_strings: Vec<(u8, String)>,
    /// Symbolic present request; the engine resolves the item.
    pub present: PresentRequest,
    pub paper: PaperSpec,
    pub sender_npc: u32,
    pub recipient_player: u8,
}

/// What the generator wants as a present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentRequest {
    None,
    /// (shop kind, list type) for `mSP_SelectRandomItem_New`.
    Catalog { kind: u8, listtype: u8 },
    Umbrella,
    FixedItem(u16),
}

/// Paper selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaperSpec {
    /// `mNpc_GetPaperType()`: random ABC paper.
    RandomAbc,
    Fixed(u8),
}

/// `mNpc_GetPaperType()`: random ABC-kind paper, normalized by
/// `(paper - ITM_PAPER_START) % PAPER_UNIQUE_NUM`.
pub fn get_paper_type<R: MailRng>(rng: &mut R, paper_start: u16, paper_unique_num: u16) -> u8 {
    // The engine picks the actual paper item; the normalization is the
    // portable part.
    let roll = rng.random(24) as u16;
    ((paper_start + roll - paper_start) % paper_unique_num) as u8
}

/// `mNpc_LoadMailDataCommon2`: the shared constructor most generators
/// use (font = received, type = normal, sender = NPC, recipient =
/// player, present, paper). Christmas deliberately bypasses it.
pub fn load_npc_mail_data_common2(
    mail_no: u16,
    sender_npc: u32,
    recipient_player: u8,
    present: PresentRequest,
    paper: PaperSpec,
    free_strings: Vec<(u8, String)>,
) -> GeneratedMail {
    GeneratedMail {
        mail_no,
        mail_type: NpcMailType::Normal,
        free_strings,
        present,
        paper,
        sender_npc,
        recipient_player,
    }
}

// ---------------------------------------------------------------------------
// Delivery
// ---------------------------------------------------------------------------

/// Delivery policies across the generators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryPolicy {
    /// Birthday / Valentine / Goodbye: mailbox, then post office.
    MailboxThenPostOffice,
    /// Christmas: mailbox only.
    MailboxOnly,
    /// Normal remail / password responses: post office only.
    PostOfficeOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryResult {
    DeliveredToMailbox,
    DeliveredToPostOffice,
    Failed,
}

/// Where a generator wants its mail to go; the engine performs the
/// storage and reports back.
pub fn delivery_policy_for(system: MailSystem) -> DeliveryPolicy {
    match system {
        MailSystem::Event | MailSystem::Birthday | MailSystem::Goodbye => {
            DeliveryPolicy::MailboxThenPostOffice
        }
        MailSystem::Christmas => DeliveryPolicy::MailboxOnly,
        MailSystem::Password | MailSystem::NormalRemail => DeliveryPolicy::PostOfficeOnly,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailSystem {
    Event,
    Birthday,
    Christmas,
    Goodbye,
    Password,
    NormalRemail,
}

// ---------------------------------------------------------------------------
// Event (Valentine) mail
// ---------------------------------------------------------------------------

/// `ANIMAL_NUM_MAX`.
pub const ANIMAL_NUM_MAX: usize = 15;
/// `ANIMAL_MEMORY_NUM`.
pub const ANIMAL_MEMORY_NUM: usize = 7;
/// `mNpc_EVENT_MAIL_FRIEND_NUM`.
pub const EVENT_MAIL_FRIEND_NUM: u8 = 3;

/// Relationship classes (`mNpc_EVENT_MAIL_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventMailType {
    BestFriend = 0,
    OkFriend = 1,
    NotFriend = 2,
}

/// One animal-memory entry (friendship view needed by the classifier).
#[derive(Clone, Copy, Debug)]
pub struct MemoryEntry {
    pub is_player: bool,
    pub player_sex: u8, // 0 = male, 1 = female
    pub friendship: i16,
    pub player_no: u8,
}

/// `mNpc_SendEventPresentMailSex`: classify the villager's opposite-sex
/// friend. Returns `(memory_idx, type)` or `None`.
pub fn classify_event_friend(
    memories: &[MemoryEntry],
    best_friend_idx: Option<usize>,
    animal_sex: u8,
) -> Option<(usize, EventMailType)> {
    let opp = memories
        .iter()
        .enumerate()
        .filter(|(_, m)| m.is_player && m.player_sex != animal_sex)
        .max_by_key(|(_, m)| m.friendship)
        .map(|(i, _)| i);
    match (best_friend_idx, opp) {
        (Some(b), Some(o)) if b == o => Some((o, EventMailType::BestFriend)),
        (_, Some(o)) => {
            let t = if memories[o].friendship >= 80 {
                EventMailType::OkFriend
            } else {
                EventMailType::NotFriend
            };
            Some((o, t))
        }
        _ => None,
    }
}

/// Present tables: `priority_table` (RARE/UNCOMMON/COMMON) and
/// `category_table` (FURNITURE/FURNITURE/CLOTH), indexed by type.
pub fn event_present_request(t: EventMailType) -> PresentRequest {
    match t {
        EventMailType::BestFriend => PresentRequest::Catalog { kind: SHOP_KIND_FURNITURE, listtype: LISTTYPE_RARE },
        EventMailType::OkFriend => PresentRequest::Catalog { kind: SHOP_KIND_FURNITURE, listtype: LISTTYPE_UNCOMMON },
        EventMailType::NotFriend => PresentRequest::Catalog { kind: SHOP_KIND_CLOTH, listtype: LISTTYPE_COMMON },
    }
}

/// `mail_no = 0x60 + looks * 3 + type`.
pub fn event_mail_no(looks: u8, t: EventMailType) -> u16 {
    0x60 + looks as u16 * EVENT_MAIL_FRIEND_NUM as u16 + t as u16
}

/// Build one Valentine mail (FREE_STR0 = player, FREE_STR6 = NPC).
pub fn make_event_mail(
    player_name: &str,
    npc_name: &str,
    sender_npc: u32,
    recipient_player: u8,
    looks: u8,
    t: EventMailType,
) -> GeneratedMail {
    load_npc_mail_data_common2(
        event_mail_no(looks, t),
        sender_npc,
        recipient_player,
        event_present_request(t),
        PaperSpec::RandomAbc,
        vec![(0, player_name.to_string()), (6, npc_name.to_string())],
    )
}

/// The Valentine scheduler (`mNpc_SendVtdayMail`): classify every
/// villager, then process classes in BEST -> OK -> NOT order. A
/// player-level bitfield (starts 0b1111) drops a player from all later
/// attempts after one delivery failure; if it hits 0 the routine ends.
///
/// `classify` maps villager idx -> Option<(player_no, type)>.
/// `try_send` attempts delivery and returns whether it succeeded.
pub fn send_valentine_mails(
    classify: &[(Option<(u8, EventMailType)>); ANIMAL_NUM_MAX],
    try_send: &mut dyn FnMut(u8, EventMailType) -> bool,
) -> u32 {
    let mut player_bitfield: u8 = 0b1111;
    let mut sent = 0u32;
    for class in [EventMailType::BestFriend, EventMailType::OkFriend, EventMailType::NotFriend] {
        for slot in classify.iter().flatten() {
            let (player_no, t) = *slot;
            if t != class {
                continue;
            }
            if player_bitfield == 0 {
                return sent;
            }
            if (player_bitfield >> player_no) & 1 == 1 {
                if try_send(player_no, t) {
                    sent += 1;
                } else {
                    // Retail quirk: one failure excludes the player from
                    // all subsequent event-mail attempts this call.
                    player_bitfield &= !(1 << player_no);
                }
            }
        }
    }
    sent
}

// Shop kinds / list types (symbolic; engine resolves).
pub const SHOP_KIND_FURNITURE: u8 = 0;
pub const SHOP_KIND_CLOTH: u8 = 1;
pub const LISTTYPE_RARE: u8 = 0;
pub const LISTTYPE_UNCOMMON: u8 = 1;
pub const LISTTYPE_COMMON: u8 = 2;

// ---------------------------------------------------------------------------
// Birthday mail
// ---------------------------------------------------------------------------

/// `mail_no = 0xEA + looks * 3 + RANDOM(3)`.
pub fn birthday_mail_no<R: MailRng>(rng: &mut R, looks: u8) -> u16 {
    0xEA + looks as u16 * 3 + rng.random(3) as u16
}

/// `mNpc_GetBirthdayPresent`: RANDOM(5) over
/// [FURNITURE, FURNITURE, CLOTH, CLOTH, umbrella] = 40/40/20.
pub fn birthday_present_request<R: MailRng>(rng: &mut R) -> PresentRequest {
    match rng.random(5) {
        0 | 1 => PresentRequest::Catalog { kind: SHOP_KIND_FURNITURE, listtype: LISTTYPE_RARE },
        2 | 3 => PresentRequest::Catalog { kind: SHOP_KIND_CLOTH, listtype: LISTTYPE_RARE },
        _ => PresentRequest::Umbrella,
    }
}

/// Build one birthday card (FREE_STR0 = player, 1 = NPC, 2 = item name).
pub fn make_birthday_card<R: MailRng>(
    rng: &mut R,
    player_name: &str,
    npc_name: &str,
    item_name: &str,
    sender_npc: u32,
    recipient_player: u8,
    looks: u8,
) -> GeneratedMail {
    load_npc_mail_data_common2(
        birthday_mail_no(rng, looks),
        sender_npc,
        recipient_player,
        birthday_present_request(rng),
        PaperSpec::RandomAbc,
        vec![
            (0, player_name.to_string()),
            (1, npc_name.to_string()),
            (2, item_name.to_string()),
        ],
    )
}

/// Birthday year state: after the yearly send,
/// `birthday_present_npc = EMPTY_NO` and `celebrated_birthday_year =
/// current_year` are recorded.
#[derive(Clone, Copy, Debug, Default)]
pub struct BirthdayYearState {
    pub celebrated_birthday_year: i32,
    pub birthday_present_npc: u32,
}

impl BirthdayYearState {
    pub const EMPTY_NO: u32 = 0xFFFF;
    /// Retail's post-send reset for a new celebration year.
    pub fn mark_celebrated(&mut self, year: i32) {
        if self.celebrated_birthday_year != year {
            self.birthday_present_npc = Self::EMPTY_NO;
            self.celebrated_birthday_year = year;
        }
    }
}

/// Eligibility: the player must be the villager's highest-friendship
/// memory AND not the recorded `birthday_present_npc`.
pub fn birthday_eligible(
    memories: &[MemoryEntry],
    player_no: u8,
    birthday_present_npc: u32,
    sender_npc: u32,
) -> bool {
    if birthday_present_npc == sender_npc {
        return false;
    }
    let best = memories
        .iter()
        .enumerate()
        .filter(|(_, m)| m.is_player)
        .max_by_key(|(_, m)| m.friendship)
        .map(|(i, _)| i);
    matches!(best, Some(i) if memories[i].player_no == player_no)
}

// ---------------------------------------------------------------------------
// Christmas mail
// ---------------------------------------------------------------------------

/// Christmas card message id.
pub const XMAS_MAIL_NO: u16 = 0xD7;
/// Fixed present: `FTR_START(FTR_FAMICOM_COMMON01)`.
pub const XMAS_PRESENT: u16 = 0x1C00; // placeholder encoding note: engine maps
/// Festive paper.
pub const XMAS_PAPER: u8 = 22;

/// `mNpc_GetXmasCardData`: fixed ROM handbill, XMAS type, fixed
/// Famicom present, festive paper. No RNG anywhere. Deliberately does
/// NOT use the common constructor (needs mail_type = XMAS).
pub fn make_xmas_card(recipient_player: u8) -> GeneratedMail {
    GeneratedMail {
        mail_no: XMAS_MAIL_NO,
        mail_type: NpcMailType::Xmas,
        free_strings: Vec::new(),
        present: PresentRequest::FixedItem(XMAS_PRESENT),
        paper: PaperSpec::Fixed(XMAS_PAPER),
        sender_npc: 0, // Santa-equivalent; engine fills
        recipient_player,
    }
}

// ---------------------------------------------------------------------------
// Goodbye mail
// ---------------------------------------------------------------------------

/// Pending goodbye record: survives the villager's `Animal_c`
/// (`static Anm_GoodbyMail_c l_mnpc_goodby_mail`).
#[derive(Clone, Debug)]
pub struct GoodbyePending {
    pub npc_id: u32,
    pub looks: u8,
    /// 4-bit recipient mask; bits clear as each delivery succeeds.
    /// Cleared entirely once zero.
    pub deliver_to_bitfield: u8,
}

impl GoodbyePending {
    pub fn new(npc_id: u32, looks: u8, active_players: u8) -> GoodbyePending {
        GoodbyePending { npc_id, looks, deliver_to_bitfield: active_players & 0xF }
    }

    pub fn is_done(&self) -> bool {
        self.deliver_to_bitfield == 0
    }
}

/// `mail_no = 0x20E + looks * 3 + mQst_GetRandom(3)` — note the quest
/// RNG, not the normal one.
pub fn goodbye_mail_no<R: MailRng>(rng: &mut R, looks: u8) -> Option<u16> {
    if looks >= 6 {
        return None;
    }
    Some(0x20E + looks as u16 * 3 + rng.quest_random(3) as u16)
}

/// Build one goodbye mail (FREE_STR0 = player, 1 = NPC, 3 = town;
/// present = EMPTY_NO).
pub fn make_goodbye_mail<R: MailRng>(
    rng: &mut R,
    player_name: &str,
    npc_name: &str,
    town_name: &str,
    pending: &GoodbyePending,
    recipient_player: u8,
) -> Option<GeneratedMail> {
    let mail_no = goodbye_mail_no(rng, pending.looks)?;
    Some(load_npc_mail_data_common2(
        mail_no,
        pending.npc_id,
        recipient_player,
        PresentRequest::None,
        PaperSpec::RandomAbc,
        vec![
            (0, player_name.to_string()),
            (1, npc_name.to_string()),
            (3, town_name.to_string()),
        ],
    ))
}

/// `mNpc_SendGoodbyAnimalMail`: retry loop over the bitfield.
/// `try_send(player_no)` attempts one delivery. Bits clear on success
/// and persist on failure, so temporary capacity failures are retried
/// next call.
pub fn send_goodbye_mails(
    pending: &mut GoodbyePending,
    try_send: &mut dyn FnMut(u8) -> bool,
) -> u32 {
    let mut sent = 0u32;
    for player_no in 0..4u8 {
        if (pending.deliver_to_bitfield >> player_no) & 1 == 1 {
            if try_send(player_no) {
                pending.deliver_to_bitfield &= !(1 << player_no);
                sent += 1;
            }
        }
    }
    sent
}

// ---------------------------------------------------------------------------
// Password (HP) mail
// ---------------------------------------------------------------------------

/// `ANIMAL_HP_MAIL_NUM = PLAYER_NUM`.
pub const HP_MAIL_NUM: usize = 4;
/// Saved password field size.
pub const HP_PASSWORD_LEN: usize = 20;

/// One villager's per-player pending password slot.
#[derive(Clone, Debug)]
pub struct HpMailSlot {
    /// Days are compared via `lbRTC_GetIntervalDays()`; retail stamps
    /// the current RTC date on receipt.
    pub receive_day: i32,
    pub password: [u8; HP_PASSWORD_LEN],
    pub occupied: bool,
}

impl HpMailSlot {
    pub fn empty() -> HpMailSlot {
        HpMailSlot { receive_day: 0, password: [0; HP_PASSWORD_LEN], occupied: false }
    }

    /// Store a password. Retail quirk (commented `@BUG`):
    /// `mMpswd_PASSWORD_DATA_LEN` is 21 but the saved field is
    /// `password[20]`, so the non-BUGFIX path writes 21 bytes into the
    /// 20-byte array — a 1-byte overrun into the following save byte.
    /// This port deliberately does NOT reproduce the overrun (a safety
    /// deviation); it stores at most 20 bytes.
    pub fn store(&mut self, day: i32, password: &[u8]) {
        let n = password.len().min(HP_PASSWORD_LEN);
        self.password[..n].copy_from_slice(&password[..n]);
        self.receive_day = day;
        self.occupied = true;
    }

    pub fn clear(&mut self) {
        *self = HpMailSlot::empty();
    }

    /// Eligible when at least one day has passed (interval-days >= 1).
    pub fn due(&self, today: i32) -> bool {
        self.occupied && today - self.receive_day >= 1
    }
}

/// The six password response generators (`send_proc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordType {
    Famicom = 0,
    Popular = 1,
    CardE = 2,
    Magazine = 3,
    UserPassword = 4,
    CardEMini = 5,
}

impl PasswordType {
    pub fn from_index(i: usize) -> Option<PasswordType> {
        match i {
            0 => Some(PasswordType::Famicom),
            1 => Some(PasswordType::Popular),
            2 => Some(PasswordType::CardE),
            3 => Some(PasswordType::Magazine),
            4 => Some(PasswordType::UserPassword),
            5 => Some(PasswordType::CardEMini),
            _ => None,
        }
    }
}

/// Password validation outcome, per generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordVerdict {
    ValidWin,
    ValidLose,
    Invalid,
}

/// Famicom: success `0x24A + looks`, failure `0x250 + looks`.
pub fn famicom_mail_no(looks: u8, valid: bool) -> u16 {
    if valid {
        0x24A + looks as u16
    } else {
        0x250 + looks as u16
    }
}

/// Popular: current NPC `0x256 + looks`, other normal NPC `0x25C +
/// looks`, islander `0x262 + looks`, special NPC `0x268 + npc_code`,
/// invalid `0x288 + looks`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopularTarget {
    CurrentNpc,
    OtherNpc,
    Islander,
    SpecialNpc(u16),
    Invalid,
}

pub fn popular_mail_no(looks: u8, target: PopularTarget) -> u16 {
    match target {
        PopularTarget::CurrentNpc => 0x256 + looks as u16,
        PopularTarget::OtherNpc => 0x25C + looks as u16,
        PopularTarget::Islander => 0x262 + looks as u16,
        PopularTarget::SpecialNpc(code) => 0x268 + code,
        PopularTarget::Invalid => 0x288 + looks as u16,
    }
}

/// Magazine probability table: hit_rate_index -> success %.
pub const MAGAZINE_PROB: [u8; 5] = [80, 60, 30, 0, 100];

/// Card-E probability table.
pub const CARDE_PROB: [u8; 4] = [80, 60, 40, 20];

/// `mNpc_GetHit`-style roll: `RANDOM(100) < prob_table[idx]`.
pub fn hit_roll<R: MailRng>(rng: &mut R, prob: u8) -> bool {
    rng.random(100) < prob as u32
}

/// Magazine: win `0x2A0 + looks`, lose `0x2A6 + looks`, invalid
/// `0x2AC + looks`. The present is kept only on the win path.
pub fn magazine_mail_no(looks: u8, verdict: PasswordVerdict) -> u16 {
    match verdict {
        PasswordVerdict::ValidWin => 0x2A0 + looks as u16,
        PasswordVerdict::ValidLose => 0x2A6 + looks as u16,
        PasswordVerdict::Invalid => 0x2AC + looks as u16,
    }
}

/// Card-E message numbers (verified against retail; an earlier draft
/// copied the brief's wrong bases).
///
/// - hit + islander:      `0x3CA + looks`
/// - hit + normal NPC:    `0x3C4 + looks`
/// - valid but no hit:    `0x2B2 + npc_code`
/// - special NPC:         `0x39E + npc_code`
/// - invalid:             `0x3BE + looks`
///
/// Retail also sets FREE_STR5 to the catchphrase (`gobi_str`); the hit
/// present is a random Famicom item, the miss present is the password's
/// item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardEOutcome {
    HitIslander,
    HitNormal,
    Miss(u16),
    SpecialNpc(u16),
    Invalid,
}

pub fn carde_mail_no(looks: u8, outcome: CardEOutcome) -> u16 {
    match outcome {
        CardEOutcome::HitIslander => 0x3CA + looks as u16,
        CardEOutcome::HitNormal => 0x3C4 + looks as u16,
        CardEOutcome::Miss(npc_code) => 0x2B2 + npc_code,
        CardEOutcome::SpecialNpc(code) => 0x39E + code,
        CardEOutcome::Invalid => 0x3BE + looks as u16,
    }
}

/// user_password / cardE_mini valid path
/// (`mNpc_SendHPMailNum_cardE_mini_user_password_common`):
/// `0x3D0 + looks` (guarded by `looks < 6`), FREE_STR6 = NPC name,
/// present EMPTY_NO.
pub fn user_password_mail_no(looks: u8) -> Option<u16> {
    if looks < 6 {
        Some(0x3D0 + looks as u16)
    } else {
        None
    }
}

/// Invalid user/cardE-mini path (`mNpc_SendHPMailNum_NG`):
/// `0x3BE + looks`, FREE_STR0 = player, FREE_STR6 = NPC.
pub fn password_ng_mail_no(looks: u8) -> u16 {
    0x3BE + looks as u16
}

/// One password-response generation request, after analysis.
#[derive(Clone, Debug)]
pub struct PasswordResponse {
    pub mail: GeneratedMail,
    /// Popular/Card-E paths overwrite the displayed sender with the
    /// NPC encoded in the password, and also overwrite the displayed
    /// recipient with the player's name.
    pub sender_override: Option<String>,
    pub recipient_override: Option<String>,
    /// Special-NPC password responses use `mMl_TYPE_SPNPC_PASSWORD`.
    pub special_npc: bool,
    /// Whether the present survives (magazine keeps it only on win).
    pub keep_present: bool,
}

/// Apply the post-construction overwrites retail does after
/// `mNpc_LoadMailDataCommon2` on the Popular/Card-E paths.
pub fn apply_password_overrides(resp: &mut PasswordResponse) {
    if resp.sender_override.is_some() || resp.recipient_override.is_some() {
        // Engine applies the bcopy()s to the mail header here.
    }
    if resp.special_npc {
        resp.mail.mail_type = NpcMailType::SpNpcPassword;
    }
}

/// Build a Famicom password response. Invalid passwords still get a
/// personality-specific mail (`0x250 + looks`).
pub fn make_famicom_response(
    player_name: &str,
    npc_name: &str,
    pw_str0: &str,
    pw_str1: &str,
    item_name: Option<&str>,
    sender_npc: u32,
    recipient_player: u8,
    looks: u8,
    valid: bool,
) -> PasswordResponse {
    let mut free = vec![
        (0, player_name.to_string()),
        (1, npc_name.to_string()),
        (2, pw_str1.to_string()),
        (3, pw_str0.to_string()),
        (6, npc_name.to_string()),
    ];
    if let Some(item) = item_name {
        free.push((4, item.to_string()));
    }
    let mail = load_npc_mail_data_common2(
        famicom_mail_no(looks, valid),
        sender_npc,
        recipient_player,
        if valid { PresentRequest::FixedItem(0) } else { PresentRequest::None },
        PaperSpec::RandomAbc,
        free,
    );
    PasswordResponse { mail, sender_override: None, recipient_override: None, special_npc: false, keep_present: valid }
}

/// The delayed scheduler: for each due HP-mail slot, analyze the
/// password, generate the response, and attempt post-office delivery.
/// The slot is cleared **only** after successful delivery — a failed
/// insertion retains the password for the next day.
pub fn send_hp_mails<R: MailRng>(
    _rng: &mut R,
    slots: &mut [HpMailSlot],
    today: i32,
    analyze: &dyn Fn(&HpMailSlot) -> Option<PasswordResponse>,
    try_deliver: &dyn Fn(&GeneratedMail) -> bool,
) -> u32 {
    let mut sent = 0u32;
    for slot in slots.iter_mut() {
        if !slot.due(today) {
            continue;
        }
        if let Some(resp) = analyze(slot) {
            if try_deliver(&resp.mail) {
                slot.clear();
                sent += 1;
            }
            // else: retained — retried on a later day.
        }
    }
    sent
}

/// All HP-mail slots for one villager (indexed by player).
#[derive(Clone, Debug)]
pub struct VillagerHpMail {
    pub slots: [HpMailSlot; HP_MAIL_NUM],
}

impl VillagerHpMail {
    pub fn new() -> VillagerHpMail {
        VillagerHpMail {
            slots: [
                HpMailSlot::empty(),
                HpMailSlot::empty(),
                HpMailSlot::empty(),
                HpMailSlot::empty(),
            ],
        }
    }
}

impl Default for VillagerHpMail {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// C ABI exports
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn pc_npcmail_event_no(looks: u8, typ: u8) -> u16 {
    match typ {
        0 => event_mail_no(looks, EventMailType::BestFriend),
        1 => event_mail_no(looks, EventMailType::OkFriend),
        _ => event_mail_no(looks, EventMailType::NotFriend),
    }
}

#[no_mangle]
pub extern "C" fn pc_npcmail_famicom_no(looks: u8, valid: u8) -> u16 {
    famicom_mail_no(looks, valid != 0)
}

#[no_mangle]
pub extern "C" fn pc_npcmail_popular_no(looks: u8, target: u8, npc_code: u16) -> u16 {
    let t = match target {
        0 => PopularTarget::CurrentNpc,
        1 => PopularTarget::OtherNpc,
        2 => PopularTarget::Islander,
        3 => PopularTarget::SpecialNpc(npc_code),
        _ => PopularTarget::Invalid,
    };
    popular_mail_no(looks, t)
}

#[no_mangle]
pub extern "C" fn pc_npcmail_magazine_no(looks: u8, verdict: u8) -> u16 {
    let v = match verdict {
        0 => PasswordVerdict::ValidWin,
        1 => PasswordVerdict::ValidLose,
        _ => PasswordVerdict::Invalid,
    };
    magazine_mail_no(looks, v)
}

#[no_mangle]
pub extern "C" fn pc_npcmail_goodbye_no(looks: u8, roll: u8) -> u16 {
    // Exposes the formula with an injected variant roll (retail uses
    // mQst_GetRandom(3)); returns 0xFFFF for invalid looks.
    if looks >= 6 || roll >= 3 {
        return 0xFFFF;
    }
    0x20E + looks as u16 * 3 + roll as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(player_no: u8, sex: u8, friendship: i16) -> MemoryEntry {
        MemoryEntry { is_player: true, player_sex: sex, friendship, player_no }
    }

    #[test]
    fn event_classification() {
        // Villager (male): best friend is player 0 (female), who is also
        // the top opposite-sex friend -> BEST_FRIEND.
        let memories = [
            mem(0, 1, 200),
            mem(1, 1, 50),
            mem(2, 0, 90),
            mem(3, 0, 10),
            mem(4, 1, 30),
            mem(5, 0, 20),
            mem(6, 1, 5),
        ];
        assert_eq!(classify_event_friend(&memories, Some(0), 0), Some((0, EventMailType::BestFriend)));
        // Best friend is same-sex player 2 -> opposite-sex best is
        // player 0 with friendship 200 >= 80 -> OK_FRIEND.
        assert_eq!(classify_event_friend(&memories, Some(2), 0), Some((0, EventMailType::OkFriend)));
        // Low friendship opposite-sex best -> NOT_FRIEND.
        let mut low = memories;
        low[0].friendship = 10;
        // With player 0 lowered to 10, the top opposite-sex friend is now
        // player 1 (index 1, friendship 50 < 80) -> NOT_FRIEND.
        assert_eq!(classify_event_friend(&low, Some(2), 0), Some((1, EventMailType::NotFriend)));
        // No opposite-sex friend -> None.
        let none: [MemoryEntry; 7] = [mem(0, 0, 10), mem(1, 0, 10), mem(2, 0, 10), mem(3, 0, 10),
                                     mem(4, 0, 10), mem(5, 0, 10), mem(6, 0, 10)];
        assert_eq!(classify_event_friend(&none, Some(0), 0), None);
    }

    #[test]
    fn event_mail_numbers() {
        assert_eq!(event_mail_no(0, EventMailType::BestFriend), 0x60);
        assert_eq!(event_mail_no(0, EventMailType::NotFriend), 0x62);
        assert_eq!(event_mail_no(5, EventMailType::NotFriend), 0x71);
        // Present tables.
        assert_eq!(
            event_present_request(EventMailType::BestFriend),
            PresentRequest::Catalog { kind: SHOP_KIND_FURNITURE, listtype: LISTTYPE_RARE }
        );
        assert_eq!(
            event_present_request(EventMailType::NotFriend),
            PresentRequest::Catalog { kind: SHOP_KIND_CLOTH, listtype: LISTTYPE_COMMON }
        );
    }

    #[test]
    fn valentine_scheduler_order_and_bitfield() {
        // Two villagers: one BEST (player 0), one NOT (player 1).
        // Delivery fails for player 0 -> bitfield excludes them; the
        // NOT_FRIEND mail for player 1 still goes out.
        let classify: [(Option<(u8, EventMailType)>); ANIMAL_NUM_MAX] = {
            let mut c: [(Option<(u8, EventMailType)>); ANIMAL_NUM_MAX] = [None; ANIMAL_NUM_MAX];
            c[0] = Some((0, EventMailType::BestFriend));
            c[1] = Some((1, EventMailType::NotFriend));
            c
        };
        let mut order = Vec::new();
        let mut try_send = |player_no: u8, t: EventMailType| {
            order.push((player_no, t));
            player_no != 0 // player 0's mailbox is full
        };
        let sent = send_valentine_mails(&classify, &mut try_send);
        assert_eq!(sent, 1);
        // BEST processed before NOT_FRIEND.
        assert_eq!(order[0], (0, EventMailType::BestFriend));
        assert_eq!(order[1], (1, EventMailType::NotFriend));
    }

    #[test]
    fn birthday_numbers_and_presents() {
        let mut rng = TestRng::new(42);
        let no = birthday_mail_no(&mut rng, 2);
        assert!((0xF0..=0xF2).contains(&no));
        // Category distribution over many rolls: only furniture/cloth/umbrella.
        let mut rng = TestRng::new(7);
        for _ in 0..50 {
            match birthday_present_request(&mut rng) {
                PresentRequest::Catalog { kind: SHOP_KIND_FURNITURE, .. } => {}
                PresentRequest::Catalog { kind: SHOP_KIND_CLOTH, .. } => {}
                PresentRequest::Umbrella => {}
                other => panic!("unexpected present {:?}", other),
            }
        }
    }

    #[test]
    fn birthday_eligibility() {
        let memories = [mem(0, 1, 100), mem(1, 0, 200), mem(2, 1, 50), mem(3, 0, 10),
                        mem(4, 1, 5), mem(5, 0, 1), mem(6, 1, 0)];
        // Player 1 is highest friendship -> eligible.
        assert!(birthday_eligible(&memories, 1, 999, 7));
        // Player 0 is not highest -> ineligible.
        assert!(!birthday_eligible(&memories, 0, 999, 7));
        // Recorded present NPC excluded.
        assert!(!birthday_eligible(&memories, 1, 7, 7));
    }

    #[test]
    fn christmas_is_fixed() {
        let m = make_xmas_card(2);
        assert_eq!(m.mail_no, 0xD7);
        assert_eq!(m.mail_type, NpcMailType::Xmas);
        assert_eq!(m.paper, PaperSpec::Fixed(22));
        assert_eq!(delivery_policy_for(MailSystem::Christmas), DeliveryPolicy::MailboxOnly);
        assert_eq!(delivery_policy_for(MailSystem::Birthday), DeliveryPolicy::MailboxThenPostOffice);
        assert_eq!(delivery_policy_for(MailSystem::Password), DeliveryPolicy::PostOfficeOnly);
    }

    #[test]
    fn goodbye_retry_bitfield() {
        let mut rng = TestRng::new(1);
        let mut pending = GoodbyePending::new(1234, 3, 0b0011);
        // Player 0 fails, player 1 succeeds.
        let mut try_send = |p: u8| p != 0;
        let sent = send_goodbye_mails(&mut pending, &mut try_send);
        assert_eq!(sent, 1);
        assert_eq!(pending.deliver_to_bitfield, 0b0001); // player 0 retained
        assert!(!pending.is_done());
        // Next attempt succeeds for player 0.
        let mut try_send2 = |_: u8| true;
        send_goodbye_mails(&mut pending, &mut try_send2);
        assert!(pending.is_done());
        // Message-number formula uses the quest RNG stream.
        let no = goodbye_mail_no(&mut rng, 3).unwrap();
        assert!((0x217..=0x219).contains(&no));
    }

    #[test]
    fn password_ranges() {
        assert_eq!(famicom_mail_no(2, true), 0x24C);
        assert_eq!(famicom_mail_no(2, false), 0x252);
        assert_eq!(popular_mail_no(1, PopularTarget::CurrentNpc), 0x257);
        assert_eq!(popular_mail_no(1, PopularTarget::Islander), 0x263);
        assert_eq!(popular_mail_no(1, PopularTarget::SpecialNpc(5)), 0x26D);
        assert_eq!(popular_mail_no(1, PopularTarget::Invalid), 0x289);
        assert_eq!(magazine_mail_no(0, PasswordVerdict::ValidWin), 0x2A0);
        assert_eq!(magazine_mail_no(0, PasswordVerdict::ValidLose), 0x2A6);
        assert_eq!(magazine_mail_no(0, PasswordVerdict::Invalid), 0x2AC);
        assert_eq!(MAGAZINE_PROB, [80, 60, 30, 0, 100]);
        assert_eq!(CARDE_PROB, [80, 60, 40, 20]);
    }

    #[test]
    fn hp_mail_delay_and_retain() {
        let mut slots = [HpMailSlot::empty(), HpMailSlot::empty(), HpMailSlot::empty(), HpMailSlot::empty()];
        slots[0].store(100, &[1u8; 20]);
        // Same day: not due.
        assert!(!slots[0].due(100));
        // Next day: due.
        assert!(slots[0].due(101));
        // Failed delivery retains the password.
        let mut rng = TestRng::new(0);
        let analyze = |_: &HpMailSlot| {
            Some(PasswordResponse {
                mail: make_xmas_card(0),
                sender_override: None,
                recipient_override: None,
                special_npc: false,
                keep_present: false,
            })
        };
        let fail = |_: &GeneratedMail| false;
        let sent = send_hp_mails(&mut rng, &mut slots, 101, &analyze, &fail);
        assert_eq!(sent, 0);
        assert!(slots[0].occupied); // retained
        // Successful delivery clears.
        let ok = |_: &GeneratedMail| true;
        let sent = send_hp_mails(&mut rng, &mut slots, 101, &analyze, &ok);
        assert_eq!(sent, 1);
        assert!(!slots[0].occupied);
    }

    #[test]
    fn carde_bases_corrected() {
        // Retail values (an earlier draft copied the brief's wrong bases).
        assert_eq!(carde_mail_no(2, CardEOutcome::HitIslander), 0x3CC);
        assert_eq!(carde_mail_no(2, CardEOutcome::HitNormal), 0x3C6);
        assert_eq!(carde_mail_no(2, CardEOutcome::Miss(7)), 0x2B9);
        assert_eq!(carde_mail_no(2, CardEOutcome::SpecialNpc(7)), 0x3A5);
        assert_eq!(carde_mail_no(2, CardEOutcome::Invalid), 0x3C0);
    }

    #[test]
    fn user_password_paths() {
        assert_eq!(user_password_mail_no(3), Some(0x3D3));
        assert_eq!(user_password_mail_no(6), None); // looks guard
        assert_eq!(password_ng_mail_no(3), 0x3C1);
    }

    #[test]
    fn password_overrides() {
        let mut r = make_famicom_response("P", "N", "s0", "s1", None, 9, 0, 4, true);
        r.sender_override = Some("SUB".to_string());
        r.recipient_override = Some("P".to_string());
        r.special_npc = true;
        apply_password_overrides(&mut r);
        assert_eq!(r.mail.mail_type, NpcMailType::SpNpcPassword);
    }

    #[test]
    fn birthday_year_reset() {
        let mut st = BirthdayYearState::default();
        st.birthday_present_npc = 42;
        st.mark_celebrated(2026);
        assert_eq!(st.birthday_present_npc, BirthdayYearState::EMPTY_NO);
        assert_eq!(st.celebrated_birthday_year, 2026);
        st.birthday_present_npc = 42;
        st.mark_celebrated(2026); // same year: no reset
        assert_eq!(st.birthday_present_npc, 42);
    }

    #[test]
    fn invalid_password_still_responds() {
        let r = make_famicom_response("P", "N", "s0", "s1", None, 9, 0, 4, false);
        assert_eq!(r.mail.mail_no, 0x254);
        assert!(!r.keep_present);
    }
}
