//! Seed a week of realistic IDC usage into the local SQLite DB.
//!
//! Wipes every user-data table (visits, patients, inventory, catalog, audit,
//! users, outbox, metrics, shifts), then inserts 7 days of activity ending on
//! 2026-05-20: 4 personas, 6 doctors, 6 check types + subtypes, 4 operators,
//! 12 inventory items with consumption maps, ~80 patients, ~25 visits/day with
//! a realistic mix of locked / draft / voided plus the matching consume_visit
//! inventory adjustments, operator shifts opened/closed each day, and audit
//! entries for create/lock/void/login/logout/vacuum throughout the week.
//!
//! Run from src-tauri/:
//!   cargo run --bin seed-weekly --release
//!
//! Connects to ~/.local/share/com.idc.system/idc-local.db (Linux).

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::Argon2;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, SqlitePool};
use std::error::Error;
use uuid::Uuid;

type BoxErr = Box<dyn Error + Send + Sync>;
type R<T> = Result<T, BoxErr>;

const ENTITY: &str = "unscoped";
const PASSWORD: &str = "password";
const DEVICE_RECEPTION: &str = "dev-reception-1";
const DEVICE_ACCOUNTING: &str = "dev-accounting-1";
const DEVICE_ADMIN: &str = "dev-admin-1";

// ---- Deterministic PRNG (mulberry32) ----------------------------------------

struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        t ^ (t >> 14)
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next_u32() as usize) % (hi - lo)
    }
    fn pct(&mut self, p: u32) -> bool {
        self.next_u32() % 100 < p
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.range(0, xs.len())]
    }
}

// ---- Data models ------------------------------------------------------------

#[derive(Clone)]
struct Persona {
    id: String,
    email: &'static str,
    name: &'static str,
    role: &'static str,
    device: &'static str,
}

#[derive(Clone)]
struct CheckType {
    id: String,
    name_ar: &'static str,
    name_en: &'static str,
    has_subtypes: bool,
    base_price_iqd: Option<i64>,
    dye: bool,
    report: bool,
}

#[derive(Clone)]
struct Subtype {
    id: String,
    check_type_id: String,
    name_ar: &'static str,
    name_en: &'static str,
    price_iqd: i64,
}

#[derive(Clone)]
struct Doctor {
    id: String,
    name: &'static str,
    specialty: &'static str,
}

#[derive(Clone)]
struct Pricing {
    id: String,
    doctor_id: String,
    check_type_id: String,
    cut_kind: &'static str,
    cut_value: i64,
    price_override_iqd: Option<i64>,
}

#[derive(Clone)]
struct Operator {
    id: String,
    name: &'static str,
    base_cut_per_check_iqd: i64,
}

#[derive(Clone)]
struct InvItem {
    id: String,
    name_ar: &'static str,
    name_en: &'static str,
    unit: &'static str,
    low_threshold: i64,
    starting_qty: i64,
}

#[derive(Clone)]
struct ConsumptionRule {
    id: String,
    check_type_id: String,
    subtype_id: Option<String>,
    item_id: String,
    qty: i64,
    on_dye_only: bool,
}

#[derive(Clone)]
struct Patient {
    id: String,
    name: &'static str,
}

// ---- Helpers ----------------------------------------------------------------

fn rfc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn iso_date(d: NaiveDate, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
    let nd = d.and_time(NaiveTime::from_hms_opt(hour, min, sec).unwrap());
    Utc.from_utc_datetime(&nd)
}

fn hash_password(pw: &str) -> R<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map_err(|e| format!("argon2 hash: {e}"))?
        .to_string();
    Ok(hash)
}

// ---- Compile-time data ------------------------------------------------------

const PATIENT_NAMES: &[&str] = &[
    "Ahmad Hussein",
    "Fatima Al-Rashid",
    "Mohammed Ibrahim",
    "Layla Khaled",
    "Omar Faisal",
    "Zainab Hassan",
    "Hassan Ali",
    "Mariam Nazar",
    "Yusuf Tahir",
    "Rana Saadi",
    "Ali Mahmoud",
    "Noor Karim",
    "Tariq Adnan",
    "Salma Reda",
    "Karim Nuri",
    "Huda Sabri",
    "Ibrahim Walid",
    "Aisha Mounir",
    "Sami Talib",
    "Dunya Hadi",
    "Bilal Othman",
    "Reem Naji",
    "Jamal Khalil",
    "Lina Saif",
    "Khaled Mansour",
    "Hala Mostafa",
    "Saad Rafiq",
    "Maha Yassin",
    "Faris Atef",
    "Suhad Kazem",
    "Murad Hilal",
    "Yara Fouad",
    "Adel Khaldoun",
    "Iman Rashed",
    "Walid Najib",
    "Sahar Tarek",
    "Nidal Anwar",
    "Rasha Mustapha",
    "Basim Qassim",
    "Asma Fadel",
    "Mazen Lutfi",
    "Salwa Akram",
    "Tahsin Mohsen",
    "Nadia Sami",
    "Wissam Adib",
    "Lubna Habib",
    "Ghassan Murtada",
    "Sundus Wadii",
    "Rami Salam",
    "Mona Talal",
    "Anas Faraj",
    "Wafa Naseer",
    "Sirwan Idris",
    "Hadeel Aram",
    "Jaber Saleem",
    "Diala Munir",
    "Kareem Maher",
    "Najwa Wajih",
    "Mounir Salim",
    "Abeer Khalid",
    "Sabah Hatim",
    "Lamia Razzaq",
    "Imad Sharif",
    "Areej Wasim",
    "Faisal Asad",
    "Banan Mahir",
    "Hussam Adnan",
    "Najat Walid",
    "Riad Hashim",
    "Bushra Tareq",
    "Jihad Kamal",
    "Hayat Aziz",
    "Saif Hatem",
    "Marwa Abu Bakr",
    "Bashar Jameel",
    "Hanaa Walid",
    "Munir Abdullah",
    "Roula Abbas",
    "Issam Khairy",
    "Suad Tareq",
    "Nizar Asaad",
    "Ghada Murad",
];

fn personas() -> [Persona; 4] {
    [
        Persona {
            id: "01940000-0000-7000-a001-000000000001".into(),
            email: "mariam@idc.local",
            name: "Mariam Hadi",
            role: "superadmin",
            device: DEVICE_ADMIN,
        },
        Persona {
            id: "01940000-0000-7000-a002-000000000001".into(),
            email: "asma@idc.local",
            name: "Asma Karim",
            role: "accountant",
            device: DEVICE_ACCOUNTING,
        },
        Persona {
            id: "01940000-0000-7000-a003-000000000001".into(),
            email: "mehdi@idc.local",
            name: "Mehdi Saleh",
            role: "receptionist",
            device: DEVICE_RECEPTION,
        },
        Persona {
            id: "01940000-0000-7000-a004-000000000001".into(),
            email: "sara@idc.local",
            name: "Sara Najib",
            role: "receptionist",
            device: DEVICE_RECEPTION,
        },
    ]
}

fn check_types() -> Vec<CheckType> {
    vec![
        CheckType {
            id: "01940000-0000-7000-c001-000000000001".into(),
            name_ar: "أشعة مقطعية",
            name_en: "CT Scan",
            has_subtypes: true,
            base_price_iqd: None,
            dye: true,
            report: true,
        },
        CheckType {
            id: "01940000-0000-7000-c002-000000000001".into(),
            name_ar: "رنين مغناطيسي",
            name_en: "MRI",
            has_subtypes: true,
            base_price_iqd: None,
            dye: true,
            report: true,
        },
        CheckType {
            id: "01940000-0000-7000-c003-000000000001".into(),
            name_ar: "سونار",
            name_en: "Ultrasound",
            has_subtypes: true,
            base_price_iqd: None,
            dye: false,
            report: true,
        },
        CheckType {
            id: "01940000-0000-7000-c004-000000000001".into(),
            name_ar: "أشعة سينية",
            name_en: "X-Ray",
            has_subtypes: false,
            base_price_iqd: Some(25_000),
            dye: false,
            report: true,
        },
        CheckType {
            id: "01940000-0000-7000-c005-000000000001".into(),
            name_ar: "كثافة العظام",
            name_en: "DEXA",
            has_subtypes: false,
            base_price_iqd: Some(80_000),
            dye: false,
            report: true,
        },
        CheckType {
            id: "01940000-0000-7000-c006-000000000001".into(),
            name_ar: "تخطيط القلب",
            name_en: "ECG",
            has_subtypes: false,
            base_price_iqd: Some(20_000),
            dye: false,
            report: false,
        },
    ]
}

fn subtypes(cts: &[CheckType]) -> Vec<Subtype> {
    let ct = |idx: usize| cts[idx].id.clone();
    let mut out = Vec::new();
    // CT (idx 0)
    let cs: &[(&str, &str, i64)] = &[
        ("الرأس", "Head", 60_000),
        ("الصدر", "Chest", 75_000),
        ("البطن", "Abdomen", 90_000),
        ("الحوض", "Pelvis", 85_000),
    ];
    for (i, (ar, en, p)) in cs.iter().enumerate() {
        out.push(Subtype {
            id: format!("01940000-0000-7000-d000-00000000010{i}"),
            check_type_id: ct(0),
            name_ar: ar,
            name_en: en,
            price_iqd: *p,
        });
    }
    // MRI (idx 1)
    let mr: &[(&str, &str, i64)] = &[
        ("الدماغ", "Brain", 180_000),
        ("العمود الفقري", "Spine", 200_000),
        ("الركبة", "Knee", 150_000),
    ];
    for (i, (ar, en, p)) in mr.iter().enumerate() {
        out.push(Subtype {
            id: format!("01940000-0000-7000-d000-00000000020{i}"),
            check_type_id: ct(1),
            name_ar: ar,
            name_en: en,
            price_iqd: *p,
        });
    }
    // US (idx 2)
    let us: &[(&str, &str, i64)] = &[
        ("بطن", "Abdominal", 40_000),
        ("غدة درقية", "Thyroid", 35_000),
        ("حوض", "Pelvic", 45_000),
        ("توليد", "Obstetric", 50_000),
    ];
    for (i, (ar, en, p)) in us.iter().enumerate() {
        out.push(Subtype {
            id: format!("01940000-0000-7000-d000-00000000030{i}"),
            check_type_id: ct(2),
            name_ar: ar,
            name_en: en,
            price_iqd: *p,
        });
    }
    out
}

fn doctors() -> Vec<Doctor> {
    vec![
        Doctor {
            id: "01940000-0000-7000-e001-000000000001".into(),
            name: "Dr. Ali Mahmoud",
            specialty: "Radiology",
        },
        Doctor {
            id: "01940000-0000-7000-e002-000000000001".into(),
            name: "Dr. Layla Hussein",
            specialty: "Cardiology",
        },
        Doctor {
            id: "01940000-0000-7000-e003-000000000001".into(),
            name: "Dr. Omar Faisal",
            specialty: "Orthopedics",
        },
        Doctor {
            id: "01940000-0000-7000-e004-000000000001".into(),
            name: "Dr. Zainab Khalil",
            specialty: "Pediatrics",
        },
        Doctor {
            id: "01940000-0000-7000-e005-000000000001".into(),
            name: "Dr. Hassan Reda",
            specialty: "Neurology",
        },
        Doctor {
            id: "01940000-0000-7000-e006-000000000001".into(),
            name: "Dr. Noor Saadi",
            specialty: "Internal Medicine",
        },
    ]
}

fn pricing(docs: &[Doctor], cts: &[CheckType]) -> Vec<Pricing> {
    // Doctor x check_type cut policies. Pct unless noted. Some have price overrides.
    let mk =
        |i: usize, doc: &Doctor, ct: &CheckType, kind: &'static str, val: i64, ovr: Option<i64>| {
            Pricing {
                id: format!("01940000-0000-7000-f000-{:012x}", i),
                doctor_id: doc.id.clone(),
                check_type_id: ct.id.clone(),
                cut_kind: kind,
                cut_value: val,
                price_override_iqd: ovr,
            }
        };
    let mut out = Vec::new();
    let mut i = 1usize;
    // Dr Ali (Radiology): CT 30%, MRI 25%, XR 35%
    out.push(mk(i, &docs[0], &cts[0], "pct", 30, None));
    i += 1;
    out.push(mk(i, &docs[0], &cts[1], "pct", 25, None));
    i += 1;
    out.push(mk(i, &docs[0], &cts[3], "pct", 35, None));
    i += 1;
    // Dr Layla (Cardiology): ECG 40%, US 20%
    out.push(mk(i, &docs[1], &cts[5], "pct", 40, None));
    i += 1;
    out.push(mk(i, &docs[1], &cts[2], "pct", 20, None));
    i += 1;
    // Dr Omar (Ortho): MRI fixed 30k, XR 30%, DEXA 35%
    out.push(mk(i, &docs[2], &cts[1], "fixed", 30_000, None));
    i += 1;
    out.push(mk(i, &docs[2], &cts[3], "pct", 30, None));
    i += 1;
    out.push(mk(i, &docs[2], &cts[4], "pct", 35, None));
    i += 1;
    // Dr Zainab (Peds): US 25%, XR fixed 10k
    out.push(mk(i, &docs[3], &cts[2], "pct", 25, None));
    i += 1;
    out.push(mk(i, &docs[3], &cts[3], "fixed", 10_000, None));
    i += 1;
    // Dr Hassan (Neuro): MRI 30% with price override on BASE check type
    out.push(mk(i, &docs[4], &cts[1], "pct", 30, None));
    i += 1;
    out.push(mk(i, &docs[4], &cts[0], "pct", 25, None));
    i += 1;
    // Dr Noor (Internal): all main
    out.push(mk(i, &docs[5], &cts[2], "pct", 20, None));
    i += 1;
    out.push(mk(i, &docs[5], &cts[3], "pct", 25, None));
    i += 1;
    out.push(mk(i, &docs[5], &cts[5], "pct", 35, None));
    i += 1;
    // Dr Ali also gets DEXA with a price override (premium reading fee)
    out.push(mk(i, &docs[0], &cts[4], "pct", 40, Some(95_000)));
    let _ = i + 1;
    out
}

fn operators() -> Vec<Operator> {
    vec![
        Operator {
            id: "01940000-0000-7000-a100-000000000001".into(),
            name: "Karim Mahmoud",
            base_cut_per_check_iqd: 5_000,
        },
        Operator {
            id: "01940000-0000-7000-a200-000000000001".into(),
            name: "Yusuf Tarek",
            base_cut_per_check_iqd: 5_000,
        },
        Operator {
            id: "01940000-0000-7000-a300-000000000001".into(),
            name: "Rana Salim",
            base_cut_per_check_iqd: 4_000,
        },
        Operator {
            id: "01940000-0000-7000-a400-000000000001".into(),
            name: "Talal Adnan",
            base_cut_per_check_iqd: 4_500,
        },
    ]
}

// Operator specialties: which check_types they can run.
fn operator_specialties(ops: &[Operator], cts: &[CheckType]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mk = |op: usize, ct: usize, i: usize| {
        (
            format!("01940000-0000-7000-a000-{:012x}", 0xb000 + i),
            ops[op].id.clone(),
            cts[ct].id.clone(),
        )
    };
    let mut i = 1;
    // Karim: CT, MRI
    out.push(mk(0, 0, i));
    i += 1;
    out.push(mk(0, 1, i));
    i += 1;
    // Yusuf: MRI, US, CT
    out.push(mk(1, 1, i));
    i += 1;
    out.push(mk(1, 2, i));
    i += 1;
    out.push(mk(1, 0, i));
    i += 1;
    // Rana: US, XR, ECG
    out.push(mk(2, 2, i));
    i += 1;
    out.push(mk(2, 3, i));
    i += 1;
    out.push(mk(2, 5, i));
    i += 1;
    // Talal: XR, DEXA, ECG, US
    out.push(mk(3, 3, i));
    i += 1;
    out.push(mk(3, 4, i));
    i += 1;
    out.push(mk(3, 5, i));
    i += 1;
    out.push(mk(3, 2, i));
    let _ = i + 1;
    out
}

fn inventory_items() -> Vec<InvItem> {
    vec![
        InvItem {
            id: "01940000-0000-7000-b001-000000000001".into(),
            name_ar: "صبغة تباين 200مل",
            name_en: "Contrast dye 200ml",
            unit: "bottle",
            low_threshold: 5,
            starting_qty: 40,
        },
        InvItem {
            id: "01940000-0000-7000-b002-000000000001".into(),
            name_ar: "قسطرة وريدية",
            name_en: "IV catheter",
            unit: "pcs",
            low_threshold: 20,
            starting_qty: 200,
        },
        InvItem {
            id: "01940000-0000-7000-b003-000000000001".into(),
            name_ar: "محلول ملحي 1لتر",
            name_en: "Saline 1L",
            unit: "bag",
            low_threshold: 15,
            starting_qty: 80,
        },
        InvItem {
            id: "01940000-0000-7000-b004-000000000001".into(),
            name_ar: "مناديل كحولية",
            name_en: "Alcohol wipes",
            unit: "pack",
            low_threshold: 10,
            starting_qty: 50,
        },
        InvItem {
            id: "01940000-0000-7000-b005-000000000001".into(),
            name_ar: "قفازات وسط",
            name_en: "Gloves (M)",
            unit: "box",
            low_threshold: 20,
            starting_qty: 250,
        },
        InvItem {
            id: "01940000-0000-7000-b006-000000000001".into(),
            name_ar: "ورق حراري",
            name_en: "Thermal paper roll",
            unit: "roll",
            low_threshold: 10,
            starting_qty: 60,
        },
        InvItem {
            id: "01940000-0000-7000-b007-000000000001".into(),
            name_ar: "أقطاب تخطيط",
            name_en: "ECG electrodes",
            unit: "pack",
            low_threshold: 5,
            starting_qty: 50,
        },
        InvItem {
            id: "01940000-0000-7000-b008-000000000001".into(),
            name_ar: "جل سونار 500مل",
            name_en: "Ultrasound gel 500ml",
            unit: "bottle",
            low_threshold: 5,
            starting_qty: 80,
        },
        InvItem {
            id: "01940000-0000-7000-b009-000000000001".into(),
            name_ar: "مريول رصاص",
            name_en: "Lead apron",
            unit: "pcs",
            low_threshold: 2,
            starting_qty: 6,
        },
        InvItem {
            id: "01940000-0000-7000-b00a-000000000001".into(),
            name_ar: "شاش طبي",
            name_en: "Gauze rolls",
            unit: "pack",
            low_threshold: 8,
            starting_qty: 35,
        },
        InvItem {
            id: "01940000-0000-7000-b00b-000000000001".into(),
            name_ar: "محقن 10مل",
            name_en: "Syringes 10ml",
            unit: "box",
            low_threshold: 6,
            starting_qty: 24,
        },
        InvItem {
            id: "01940000-0000-7000-b00c-000000000001".into(),
            name_ar: "مطهر 5لتر",
            name_en: "Disinfectant 5L",
            unit: "bottle",
            low_threshold: 3,
            starting_qty: 10,
        },
    ]
}

fn consumption_rules(cts: &[CheckType], items: &[InvItem]) -> Vec<ConsumptionRule> {
    // (check_type_idx, item_idx, qty, on_dye_only)
    let rules: &[(usize, usize, i64, bool)] = &[
        // CT (idx 0)
        (0, 0, 1, true),  // contrast dye on dye
        (0, 1, 1, true),  // IV catheter on dye
        (0, 2, 1, true),  // saline on dye
        (0, 4, 1, false), // gloves always
        // MRI (idx 1)
        (1, 0, 1, true),
        (1, 4, 1, false),
        // US (idx 2)
        (2, 7, 1, false), // US gel
        // XR (idx 3)
        (3, 5, 1, false), // thermal paper
        (3, 4, 1, false),
        // DEXA (idx 4)
        (4, 5, 1, false),
        (4, 4, 1, false),
        // ECG (idx 5)
        (5, 6, 1, false), // electrodes
        (5, 3, 1, false), // alcohol wipes
    ];
    rules
        .iter()
        .enumerate()
        .map(|(i, (ct_i, it_i, qty, on_dye))| ConsumptionRule {
            id: format!("01940000-0000-7000-b100-{:012x}", i + 1),
            check_type_id: cts[*ct_i].id.clone(),
            subtype_id: None,
            item_id: items[*it_i].id.clone(),
            qty: *qty,
            on_dye_only: *on_dye,
        })
        .collect()
}

fn patients() -> Vec<Patient> {
    PATIENT_NAMES
        .iter()
        .enumerate()
        .map(|(i, n)| Patient {
            id: format!("01940000-0000-7000-9000-{:012x}", i + 1),
            name: n,
        })
        .collect()
}

// ---- Wipe -------------------------------------------------------------------

async fn wipe(pool: &SqlitePool) -> R<()> {
    let stmts = [
        "DELETE FROM outbox;",
        "DELETE FROM metrics_events;",
        "DELETE FROM audit_log;",
        "DELETE FROM inventory_adjustments;",
        "DELETE FROM visits;",
        "DELETE FROM patients;",
        "DELETE FROM inventory_consumption_map;",
        "DELETE FROM inventory_items;",
        "DELETE FROM operator_shifts;",
        "DELETE FROM operator_specialties;",
        "DELETE FROM operators;",
        "DELETE FROM doctor_check_pricing;",
        "DELETE FROM doctors;",
        "DELETE FROM check_subtypes;",
        "DELETE FROM check_types;",
        "DELETE FROM users;",
    ];
    for s in stmts {
        pool.execute(s).await?;
    }
    Ok(())
}

// ---- Insert -----------------------------------------------------------------

async fn insert_users(pool: &SqlitePool, ps: &[Persona], at: DateTime<Utc>) -> R<()> {
    for p in ps {
        // Fresh hash per user so salts are unique even though the plaintext
        // password is shared across the four dev personas.
        let pw_hash = hash_password(PASSWORD)?;
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, is_active, created_at, updated_at, version, dirty, origin_device_id, entity_id) \
             VALUES (?, ?, ?, ?, ?, 1, ?, ?, 1, 0, ?, ?)",
        )
        .bind(&p.id)
        .bind(p.email)
        .bind(p.name)
        .bind(&pw_hash)
        .bind(p.role)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(p.device)
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_check_types(pool: &SqlitePool, cts: &[CheckType], at: DateTime<Utc>) -> R<()> {
    for (i, c) in cts.iter().enumerate() {
        sqlx::query(
            "INSERT INTO check_types (id, name_ar, name_en, has_subtypes, base_price_iqd, dye_supported, report_supported, sort_order, is_active, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, 1, 0, ?)",
        )
        .bind(&c.id)
        .bind(c.name_ar)
        .bind(c.name_en)
        .bind(c.has_subtypes as i32)
        .bind(c.base_price_iqd)
        .bind(c.dye as i32)
        .bind(c.report as i32)
        .bind(i as i32)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_subtypes(pool: &SqlitePool, subs: &[Subtype], at: DateTime<Utc>) -> R<()> {
    for (i, s) in subs.iter().enumerate() {
        sqlx::query(
            "INSERT INTO check_subtypes (id, check_type_id, name_ar, name_en, price_iqd, sort_order, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?)",
        )
        .bind(&s.id)
        .bind(&s.check_type_id)
        .bind(s.name_ar)
        .bind(s.name_en)
        .bind(s.price_iqd)
        .bind(i as i32)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_doctors(pool: &SqlitePool, docs: &[Doctor], at: DateTime<Utc>) -> R<()> {
    for d in docs {
        sqlx::query(
            "INSERT INTO doctors (id, name, specialty, is_active, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, 1, ?, ?, 1, 0, ?)",
        )
        .bind(&d.id)
        .bind(d.name)
        .bind(d.specialty)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_pricing(pool: &SqlitePool, prs: &[Pricing], at: DateTime<Utc>) -> R<()> {
    for p in prs {
        sqlx::query(
            "INSERT INTO doctor_check_pricing (id, doctor_id, check_type_id, price_override_iqd, cut_kind, cut_value, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?)",
        )
        .bind(&p.id)
        .bind(&p.doctor_id)
        .bind(&p.check_type_id)
        .bind(p.price_override_iqd)
        .bind(p.cut_kind)
        .bind(p.cut_value)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_operators(pool: &SqlitePool, ops: &[Operator], at: DateTime<Utc>) -> R<()> {
    for o in ops {
        sqlx::query(
            "INSERT INTO operators (id, name, base_cut_per_check_iqd, is_active, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, 1, ?, ?, 1, 0, ?)",
        )
        .bind(&o.id)
        .bind(o.name)
        .bind(o.base_cut_per_check_iqd)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_op_specialties(
    pool: &SqlitePool,
    rows: &[(String, String, String)],
    at: DateTime<Utc>,
) -> R<()> {
    for (id, op_id, ct_id) in rows {
        sqlx::query(
            "INSERT INTO operator_specialties (id, operator_id, check_type_id, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, ?, 1, 0, ?)",
        )
        .bind(id)
        .bind(op_id)
        .bind(ct_id)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_inventory_items(pool: &SqlitePool, items: &[InvItem], at: DateTime<Utc>) -> R<()> {
    for it in items {
        sqlx::query(
            "INSERT INTO inventory_items (id, name_ar, name_en, unit, quantity_on_hand, low_stock_threshold, is_active, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, 0, ?, 1, ?, ?, 1, 0, ?)",
        )
        .bind(&it.id)
        .bind(it.name_ar)
        .bind(it.name_en)
        .bind(it.unit)
        .bind(it.low_threshold)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_consumption(
    pool: &SqlitePool,
    rules: &[ConsumptionRule],
    at: DateTime<Utc>,
) -> R<()> {
    for r in rules {
        sqlx::query(
            "INSERT INTO inventory_consumption_map (id, check_type_id, check_subtype_id, item_id, quantity_per_check, on_dye_only, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?)",
        )
        .bind(&r.id)
        .bind(&r.check_type_id)
        .bind(&r.subtype_id)
        .bind(&r.item_id)
        .bind(r.qty)
        .bind(r.on_dye_only as i32)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn insert_patients(pool: &SqlitePool, ps: &[Patient], anchor: DateTime<Utc>) -> R<()> {
    let mut rng = Rng::new(99);
    for (i, p) in ps.iter().enumerate() {
        // Spread patient created_at over the prior ~60 days so the "recent
        // patients" listing has a believable shape.
        let offset_days = rng.range(1, 60) as i64;
        let at = anchor - Duration::days(offset_days) - Duration::minutes(i as i64 * 11);
        sqlx::query(
            "INSERT INTO patients (id, name, created_at, updated_at, version, dirty, entity_id) \
             VALUES (?, ?, ?, ?, 1, 0, ?)",
        )
        .bind(&p.id)
        .bind(p.name)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(ENTITY)
        .execute(pool)
        .await?;
    }
    Ok(())
}

// ---- Audit helper -----------------------------------------------------------

struct AuditWriter<'a> {
    pool: &'a SqlitePool,
    counter: u64,
}

impl<'a> AuditWriter<'a> {
    fn new(pool: &'a SqlitePool) -> Self {
        Self { pool, counter: 1 }
    }

    // Dev-only seed binary: a wide positional signature is acceptable here.
    // Tracked as a build-health follow-up to refactor into a params struct.
    #[allow(clippy::too_many_arguments)]
    async fn write(
        &mut self,
        actor: &str,
        action: &str,
        entity: &str,
        entity_id: &str,
        delta_json: &str,
        device: &str,
        at: DateTime<Utc>,
    ) -> R<()> {
        let id = Uuid::now_v7().to_string();
        let _ = self.counter;
        self.counter += 1;
        sqlx::query(
            "INSERT INTO audit_log (id, actor_user_id, action, entity, entity_id, delta, ip, device_id, at, created_at, updated_at, version, dirty, origin_device_id, entity_id_tenant) \
             VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, 1, 0, ?, ?)",
        )
        .bind(id)
        .bind(actor)
        .bind(action)
        .bind(entity)
        .bind(entity_id)
        .bind(delta_json)
        .bind(device)
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(rfc(at))
        .bind(device)
        .bind(ENTITY)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

// ---- Money math (mirrors src/domains/visits/domain/services/money_math.rs) --

struct Money {
    price: i64,
    dye_cost: i64,
    report_cost: i64,
    doctor_cut: i64,
    internal_pct: Option<i64>,
    operator_cut: i64,
    total: i64,
}

// Dev-only seed binary: a wide positional signature is acceptable here.
// Tracked as a build-health follow-up to refactor into a params struct.
#[allow(clippy::too_many_arguments)]
fn compute_money(
    ct: &CheckType,
    sub: Option<&Subtype>,
    pricing: Option<&Pricing>,
    op: &Operator,
    dye: bool,
    report: bool,
    dye_cost_setting: i64,
    report_cost_setting: i64,
    internal_pct: i64,
) -> Money {
    let base = if let Some(s) = sub {
        s.price_iqd
    } else {
        ct.base_price_iqd
            .expect("flat check type must have base price")
    };
    let price = pricing.and_then(|p| p.price_override_iqd).unwrap_or(base);
    let dye_c = if dye { dye_cost_setting } else { 0 };
    let rep_c = if report { report_cost_setting } else { 0 };
    let (doc_cut, ipct) = match pricing {
        Some(p) => {
            let c = if p.cut_kind == "pct" {
                price * p.cut_value / 100
            } else {
                p.cut_value
            };
            (c, None)
        }
        None => (price * internal_pct / 100, Some(internal_pct)),
    };
    Money {
        price,
        dye_cost: dye_c,
        report_cost: rep_c,
        doctor_cut: doc_cut,
        internal_pct: ipct,
        operator_cut: op.base_cut_per_check_iqd,
        total: price + dye_c + rep_c,
    }
}

// ---- Shifts + receive + daily visits ---------------------------------------

async fn open_shift(
    pool: &SqlitePool,
    audit: &mut AuditWriter<'_>,
    op: &Operator,
    by_user: &Persona,
    at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> R<()> {
    let id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO operator_shifts (id, operator_id, check_in_at, check_out_at, check_in_by_user_id, check_out_by_user_id, created_at, updated_at, version, dirty, origin_device_id, entity_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 2, 0, ?, ?)",
    )
    .bind(&id)
    .bind(&op.id)
    .bind(rfc(at))
    .bind(rfc(end_at))
    .bind(&by_user.id)
    .bind(&by_user.id)
    .bind(rfc(at))
    .bind(rfc(end_at))
    .bind(by_user.device)
    .bind(ENTITY)
    .execute(pool)
    .await?;
    audit
        .write(
            &by_user.id,
            "clock_in",
            "operator_shifts",
            &id,
            &format!("{{\"operator_id\":\"{}\"}}", op.id),
            by_user.device,
            at,
        )
        .await?;
    audit
        .write(
            &by_user.id,
            "clock_out",
            "operator_shifts",
            &id,
            &format!("{{\"operator_id\":\"{}\"}}", op.id),
            by_user.device,
            end_at,
        )
        .await?;
    Ok(())
}

// Dev-only seed binary: a wide positional signature is acceptable here.
// Tracked as a build-health follow-up to refactor into a params struct.
#[allow(clippy::too_many_arguments)]
async fn adjust_inventory(
    pool: &SqlitePool,
    audit: &mut AuditWriter<'_>,
    item: &InvItem,
    delta: i64,
    reason: &str,
    visit_id: Option<&str>,
    by_user: &Persona,
    at: DateTime<Utc>,
) -> R<()> {
    let id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO inventory_adjustments (id, item_id, delta, reason, visit_id, note, by_user_id, created_at, updated_at, version, dirty, origin_device_id, entity_id) \
         VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, 1, 0, ?, ?)",
    )
    .bind(&id)
    .bind(&item.id)
    .bind(delta)
    .bind(reason)
    .bind(visit_id)
    .bind(&by_user.id)
    .bind(rfc(at))
    .bind(rfc(at))
    .bind(by_user.device)
    .bind(ENTITY)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE inventory_items SET quantity_on_hand = quantity_on_hand + ?, updated_at = ?, version = version + 1 WHERE id = ?")
        .bind(delta)
        .bind(rfc(at))
        .bind(&item.id)
        .execute(pool)
        .await?;
    let delta_json = format!("{{\"delta\":{},\"reason\":\"{}\"}}", delta, reason);
    audit
        .write(
            &by_user.id,
            "create",
            "inventory_adjustments",
            &id,
            &delta_json,
            by_user.device,
            at,
        )
        .await?;
    Ok(())
}

// ---- Visit creation ---------------------------------------------------------

struct VisitContext<'a> {
    cts: &'a [CheckType],
    subs: &'a [Subtype],
    docs: &'a [Doctor],
    pricing: &'a [Pricing],
    ops: &'a [Operator],
    op_specs: &'a [(String, String, String)],
    items: &'a [InvItem],
    rules: &'a [ConsumptionRule],
    patients: &'a [Patient],
    receps: &'a [Persona],
    admin: &'a Persona,
    dye_cost: i64,
    report_cost: i64,
    internal_pct: i64,
}

#[allow(clippy::too_many_arguments)]
async fn create_visit(
    pool: &SqlitePool,
    audit: &mut AuditWriter<'_>,
    ctx: &VisitContext<'_>,
    rng: &mut Rng,
    day: NaiveDate,
    seq_in_day: usize,
    status: &'static str, // "locked" | "draft" | "voided"
) -> R<()> {
    // Pick check type. Bias towards US (cheap + frequent) and CT.
    let ct_weights = [3, 2, 4, 3, 1, 2]; // matches check_types order
    let mut wsum = 0u32;
    for w in &ct_weights {
        wsum += *w as u32;
    }
    let mut pick = rng.next_u32() % wsum;
    let mut ct_idx = 0;
    for (i, w) in ct_weights.iter().enumerate() {
        if pick < *w as u32 {
            ct_idx = i;
            break;
        }
        pick -= *w as u32;
    }
    let ct = &ctx.cts[ct_idx];

    // Subtype if required.
    let sub = if ct.has_subtypes {
        let cands: Vec<&Subtype> = ctx
            .subs
            .iter()
            .filter(|s| s.check_type_id == ct.id)
            .collect();
        Some(*rng.pick(&cands))
    } else {
        None
    };

    // Operator: must have specialty for this check type.
    let op_candidates: Vec<&Operator> = ctx
        .ops
        .iter()
        .filter(|o| {
            ctx.op_specs
                .iter()
                .any(|(_, op_id, ct_id)| op_id == &o.id && ct_id == &ct.id)
        })
        .collect();
    let op = if op_candidates.is_empty() {
        &ctx.ops[0]
    } else {
        *rng.pick(&op_candidates)
    };

    // Doctor: 70% of visits have a doctor; pick one that has pricing for this check type.
    let with_doctor = rng.pct(70);
    let (doctor, pricing) = if with_doctor {
        let doc_candidates: Vec<(&Doctor, &Pricing)> = ctx
            .pricing
            .iter()
            .filter(|p| p.check_type_id == ct.id)
            .filter_map(|p| {
                ctx.docs
                    .iter()
                    .find(|d| d.id == p.doctor_id)
                    .map(|d| (d, p))
            })
            .collect();
        if doc_candidates.is_empty() {
            (None, None)
        } else {
            let pair = rng.pick(&doc_candidates);
            (Some(pair.0), Some(pair.1))
        }
    } else {
        (None, None)
    };

    let dye = ct.dye && rng.pct(35);
    let report = ct.report && rng.pct(65);
    let patient = rng.pick(ctx.patients);
    let recep = rng.pick(ctx.receps);
    let id = Uuid::now_v7().to_string();

    let created_at = iso_date(
        day,
        8 + (seq_in_day as u32 / 4),
        (seq_in_day as u32 * 7) % 60,
        (seq_in_day as u32 * 13) % 60,
    );

    let money = compute_money(
        ct,
        sub,
        pricing,
        op,
        dye,
        report,
        ctx.dye_cost,
        ctx.report_cost,
        ctx.internal_pct,
    );

    match status {
        "draft" => {
            sqlx::query(
                "INSERT INTO visits (id, patient_id, status, receptionist_user_id, check_type_id, check_subtype_id, doctor_id, operator_id, dye, report, created_at, updated_at, version, dirty, origin_device_id, entity_id) \
                 VALUES (?, ?, 'draft', ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?, ?)",
            )
            .bind(&id).bind(&patient.id).bind(&recep.id).bind(&ct.id)
            .bind(sub.map(|s| s.id.clone())).bind(doctor.map(|d| d.id.clone())).bind(&op.id)
            .bind(dye as i32).bind(report as i32)
            .bind(rfc(created_at)).bind(rfc(created_at))
            .bind(recep.device).bind(ENTITY)
            .execute(pool).await?;
            audit
                .write(
                    &recep.id,
                    "create",
                    "visits",
                    &id,
                    "{\"status\":\"draft\"}",
                    recep.device,
                    created_at,
                )
                .await?;
        }
        "locked" | "voided" => {
            let lock_at = created_at + Duration::minutes(rng.range(2, 15) as i64);
            sqlx::query(
                "INSERT INTO visits (id, patient_id, status, receptionist_user_id, check_type_id, check_subtype_id, doctor_id, operator_id, dye, report, locked_at, \
                  price_snapshot_iqd, dye_cost_snapshot_iqd, report_cost_snapshot_iqd, doctor_cut_snapshot_iqd, operator_cut_snapshot_iqd, internal_pct_snapshot, total_amount_iqd_snapshot, \
                  patient_name_snapshot, doctor_name_snapshot, operator_name_snapshot, check_type_name_ar_snapshot, check_type_name_en_snapshot, check_subtype_name_ar_snapshot, check_subtype_name_en_snapshot, \
                  created_at, updated_at, version, dirty, origin_device_id, entity_id) \
                 VALUES (?, ?, 'locked', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 2, 0, ?, ?)",
            )
            .bind(&id).bind(&patient.id).bind(&recep.id).bind(&ct.id)
            .bind(sub.map(|s| s.id.clone())).bind(doctor.map(|d| d.id.clone())).bind(&op.id)
            .bind(dye as i32).bind(report as i32).bind(rfc(lock_at))
            .bind(money.price).bind(money.dye_cost).bind(money.report_cost)
            .bind(money.doctor_cut).bind(money.operator_cut).bind(money.internal_pct).bind(money.total)
            .bind(patient.name)
            .bind(doctor.map(|d| d.name))
            .bind(op.name)
            .bind(ct.name_ar).bind(ct.name_en)
            .bind(sub.map(|s| s.name_ar)).bind(sub.map(|s| s.name_en))
            .bind(rfc(created_at)).bind(rfc(lock_at))
            .bind(recep.device).bind(ENTITY)
            .execute(pool).await?;
            audit
                .write(
                    &recep.id,
                    "create",
                    "visits",
                    &id,
                    "{\"status\":\"draft\"}",
                    recep.device,
                    created_at,
                )
                .await?;
            audit
                .write(
                    &recep.id,
                    "lock",
                    "visits",
                    &id,
                    &format!("{{\"total\":{}}}", money.total),
                    recep.device,
                    lock_at,
                )
                .await?;

            // Apply consume_visit inventory adjustments per consumption rules.
            for r in ctx.rules {
                if r.check_type_id != ct.id {
                    continue;
                }
                if r.on_dye_only && !dye {
                    continue;
                }
                let item = ctx
                    .items
                    .iter()
                    .find(|i| i.id == r.item_id)
                    .expect("item must exist for rule");
                adjust_inventory(
                    pool,
                    audit,
                    item,
                    -r.qty,
                    "consume_visit",
                    Some(&id),
                    recep,
                    lock_at,
                )
                .await?;
            }

            if status == "voided" {
                let void_at = lock_at + Duration::hours(rng.range(1, 6) as i64);
                let reason = "Incorrect patient identification";
                sqlx::query(
                    "UPDATE visits SET status='voided', voided_at=?, voided_by_user_id=?, void_reason=?, updated_at=?, version=version+1 WHERE id=?",
                )
                .bind(rfc(void_at))
                .bind(&ctx.admin.id)
                .bind(reason)
                .bind(rfc(void_at))
                .bind(&id)
                .execute(pool).await?;
                audit
                    .write(
                        &ctx.admin.id,
                        "void",
                        "visits",
                        &id,
                        &format!("{{\"reason\":\"{}\"}}", reason),
                        ctx.admin.device,
                        void_at,
                    )
                    .await?;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

// ---- Driver -----------------------------------------------------------------

#[tokio::main]
async fn main() -> R<()> {
    let home = std::env::var("HOME")?;
    let db_path = format!("{home}/.local/share/com.idc.system/idc-local.db");
    eprintln!("[seed-weekly] connecting to {db_path}");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{db_path}"))
        .await?;

    pool.execute("PRAGMA foreign_keys = ON;").await?;

    eprintln!("[seed-weekly] wiping user-data tables");
    wipe(&pool).await?;

    // Anchor: last second of 2026-05-20 (today per current memory).
    let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
    let week_start = today - Duration::days(6); // Thu 2026-05-14
    let catalog_at = iso_date(week_start - Duration::days(60), 8, 0, 0); // 60 days ago

    let ps = personas();
    let cts = check_types();
    let subs = subtypes(&cts);
    let docs = doctors();
    let prs = pricing(&docs, &cts);
    let ops = operators();
    let op_specs = operator_specialties(&ops, &cts);
    let items = inventory_items();
    let rules = consumption_rules(&cts, &items);
    let pts = patients();

    eprintln!("[seed-weekly] seeding users + catalog");
    insert_users(&pool, &ps, catalog_at).await?;
    insert_check_types(&pool, &cts, catalog_at).await?;
    insert_subtypes(&pool, &subs, catalog_at).await?;
    insert_doctors(&pool, &docs, catalog_at).await?;
    insert_pricing(&pool, &prs, catalog_at).await?;
    insert_operators(&pool, &ops, catalog_at).await?;
    insert_op_specialties(&pool, &op_specs, catalog_at).await?;
    insert_inventory_items(&pool, &items, catalog_at).await?;
    insert_consumption(&pool, &rules, catalog_at).await?;
    insert_patients(&pool, &pts, iso_date(today, 0, 0, 0)).await?;

    let mut audit = AuditWriter::new(&pool);

    // Catalog creation audit entries (one per kind, batched).
    let admin = &ps[0];
    audit
        .write(
            &admin.id,
            "create",
            "users",
            &admin.id,
            "{\"role\":\"superadmin\"}",
            admin.device,
            catalog_at,
        )
        .await?;
    for c in &cts {
        audit
            .write(
                &admin.id,
                "create",
                "check_types",
                &c.id,
                &format!("{{\"name_en\":\"{}\"}}", c.name_en),
                admin.device,
                catalog_at,
            )
            .await?;
    }
    for d in &docs {
        audit
            .write(
                &admin.id,
                "create",
                "doctors",
                &d.id,
                &format!("{{\"name\":\"{}\"}}", d.name),
                admin.device,
                catalog_at,
            )
            .await?;
    }
    for o in &ops {
        audit
            .write(
                &admin.id,
                "create",
                "operators",
                &o.id,
                &format!("{{\"name\":\"{}\"}}", o.name),
                admin.device,
                catalog_at,
            )
            .await?;
    }

    // Initial stock receive for every inventory item (60 days back).
    eprintln!("[seed-weekly] seeding initial inventory receive");
    for it in &items {
        adjust_inventory(
            &pool,
            &mut audit,
            it,
            it.starting_qty,
            "receive",
            None,
            admin,
            catalog_at + Duration::hours(1),
        )
        .await?;
    }

    // 7 days of activity.
    let ctx = VisitContext {
        cts: &cts,
        subs: &subs,
        docs: &docs,
        pricing: &prs,
        ops: &ops,
        op_specs: &op_specs,
        items: &items,
        rules: &rules,
        patients: &pts,
        receps: &ps[2..4], // Mehdi + Sara
        admin: &ps[0],
        dye_cost: 10_000,
        report_cost: 10_000,
        internal_pct: 30,
    };

    let mut rng = Rng::new(1);

    for day_offset in 0..7 {
        let day = week_start + Duration::days(day_offset);
        eprintln!("[seed-weekly] day {} ({})", day_offset + 1, day);

        // Logins (08:00 for receptionists, 08:30 for accountant, 09:00 admin spot-check)
        audit
            .write(
                &ps[2].id,
                "login",
                "users",
                &ps[2].id,
                "{}",
                ps[2].device,
                iso_date(day, 8, 0, 0),
            )
            .await?;
        audit
            .write(
                &ps[3].id,
                "login",
                "users",
                &ps[3].id,
                "{}",
                ps[3].device,
                iso_date(day, 8, 5, 0),
            )
            .await?;
        audit
            .write(
                &ps[1].id,
                "login",
                "users",
                &ps[1].id,
                "{}",
                ps[1].device,
                iso_date(day, 8, 30, 0),
            )
            .await?;
        if day_offset % 2 == 0 {
            audit
                .write(
                    &ps[0].id,
                    "login",
                    "users",
                    &ps[0].id,
                    "{}",
                    ps[0].device,
                    iso_date(day, 9, 0, 0),
                )
                .await?;
        }

        // Operator shifts: open ~08:15, close ~17:30 (all four operators most days, one off mid-week)
        let off_op = if day_offset == 3 { Some(1usize) } else { None };
        for (i, op) in ops.iter().enumerate() {
            if Some(i) == off_op {
                continue;
            }
            let in_at = iso_date(day, 8, 10 + (i as u32 * 3), 0);
            let out_at = iso_date(day, 17, 30 + (i as u32 * 2), 0);
            let by = &ps[2 + (i % 2)]; // alternate between Mehdi and Sara
            open_shift(&pool, &mut audit, op, by, in_at, out_at).await?;
        }

        // Periodic receive: every other day a couple items get restock.
        if day_offset % 2 == 1 {
            adjust_inventory(
                &pool,
                &mut audit,
                &items[1],
                50,
                "receive",
                None,
                &ps[2],
                iso_date(day, 9, 30, 0),
            )
            .await?;
            adjust_inventory(
                &pool,
                &mut audit,
                &items[3],
                20,
                "receive",
                None,
                &ps[2],
                iso_date(day, 9, 35, 0),
            )
            .await?;
            adjust_inventory(
                &pool,
                &mut audit,
                &items[0],
                10,
                "receive",
                None,
                &ps[2],
                iso_date(day, 9, 40, 0),
            )
            .await?;
        }

        // Visit count: 22..32 per day, biased lighter on Friday (idx 1 since week_start=Thu)
        let weekday = day.format("%a").to_string();
        let count = if weekday == "Fri" {
            rng.range(14, 20)
        } else {
            rng.range(22, 32)
        };

        // Mix: 70% locked, 15% voided (admin acts), 15% draft (still in workspace)
        // For non-today days, drafts become very rare (we'd expect them to be locked already);
        // for today, drafts are still common.
        let (lock_pct, void_pct) = if day_offset < 6 {
            (90u32, 5u32) // older days: most locked, few voided, ~5% draft
        } else {
            (60u32, 5u32) // today: more drafts in workspace
        };

        for seq in 0..count {
            let roll = rng.next_u32() % 100;
            let status = if roll < lock_pct {
                "locked"
            } else if roll < lock_pct + void_pct {
                "voided"
            } else {
                "draft"
            };
            create_visit(&pool, &mut audit, &ctx, &mut rng, day, seq, status).await?;
        }

        // Sometimes a writeoff (broken vial, expired stock).
        if day_offset == 2 {
            adjust_inventory(
                &pool,
                &mut audit,
                &items[0],
                -1,
                "writeoff",
                None,
                &ps[1],
                iso_date(day, 14, 0, 0),
            )
            .await?;
        }
        if day_offset == 5 {
            adjust_inventory(
                &pool,
                &mut audit,
                &items[7],
                -1,
                "writeoff",
                None,
                &ps[1],
                iso_date(day, 15, 0, 0),
            )
            .await?;
        }
        // A count correction once during the week.
        if day_offset == 4 {
            adjust_inventory(
                &pool,
                &mut audit,
                &items[10],
                2,
                "count_correction",
                None,
                &ps[1],
                iso_date(day, 16, 30, 0),
            )
            .await?;
        }

        // End of day: vacuum audit + logouts.
        audit
            .write(
                "00000000-0000-0000-0000-000000000000",
                "vacuum",
                "audit_log",
                "00000000-0000-0000-0000-000000000000",
                "{\"mode\":\"daily\"}",
                "system",
                iso_date(day, 23, 0, 0),
            )
            .await?;
        audit
            .write(
                &ps[2].id,
                "logout",
                "users",
                &ps[2].id,
                "{}",
                ps[2].device,
                iso_date(day, 18, 0, 0),
            )
            .await?;
        audit
            .write(
                &ps[3].id,
                "logout",
                "users",
                &ps[3].id,
                "{}",
                ps[3].device,
                iso_date(day, 18, 30, 0),
            )
            .await?;
        audit
            .write(
                &ps[1].id,
                "logout",
                "users",
                &ps[1].id,
                "{}",
                ps[1].device,
                iso_date(day, 17, 0, 0),
            )
            .await?;
    }

    // Update sync_state to look freshly synced.
    let now = iso_date(today, 19, 0, 0);
    sqlx::query("UPDATE sync_state SET last_pulled_at = ?, last_pushed_at = ?, last_audit_vacuum_at = ? WHERE id = 1")
        .bind(rfc(now))
        .bind(rfc(now))
        .bind(rfc(now))
        .execute(&pool)
        .await?;

    // Mark every seeded row as clean (dirty=0) and assign a recent last_synced_at
    // so the dashboard's "needs sync" pill stays calm. (Audit log entries already 0).
    for tbl in [
        "users",
        "settings",
        "check_types",
        "check_subtypes",
        "doctors",
        "doctor_check_pricing",
        "operators",
        "operator_specialties",
        "inventory_items",
        "inventory_consumption_map",
        "patients",
        "visits",
        "operator_shifts",
        "inventory_adjustments",
    ] {
        let sql = format!("UPDATE {tbl} SET dirty = 0, last_synced_at = ?");
        sqlx::query(&sql).bind(rfc(now)).execute(&pool).await?;
    }

    // Summary.
    for t in [
        "users",
        "check_types",
        "check_subtypes",
        "doctors",
        "doctor_check_pricing",
        "operators",
        "operator_specialties",
        "inventory_items",
        "inventory_consumption_map",
        "patients",
        "visits",
        "operator_shifts",
        "inventory_adjustments",
        "audit_log",
    ] {
        let row: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {t}"))
            .fetch_one(&pool)
            .await?;
        eprintln!("[seed-weekly]  {t}: {}", row.0);
    }

    eprintln!("[seed-weekly] done");
    Ok(())
}
