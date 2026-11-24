// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The `radioFoundry` block of a qdmr YAML file (RadioFoundry `docs/spec/qdmr-yaml.md` 3.7): what qdmr has no element
// for, carried in keys qdmr ignores. A top-level block holds the systems and the kinds' radio-wide records; a channel's
// block holds its mode and its Accept Invalid CRC setting. Ported from `site/static/js/codeplug/qdmrmodes.js`.
// Fields carry the names the web editor's mode modules give them, in the order of their fresh sets. Keys (a system's
// secret, the traffic keys) are never written.

use crate::model::Codeplug;
use crate::modes::{
    Fallback, Params, KIND_NXDN, KIND_P25, KIND_P25_TRUNK, KIND_T3,
};
use crate::qdmr::{add, frequency_text, Note, Notes};
use crate::records::Channel;
use crate::systems::{
    EntryBody, SysEntry, System, SystemFields, SHARE_IDENTITY, SHARE_OFF, SHARE_WHOLE, SYSTEM_NXDN,
    SYSTEM_P25_TRUNK, SYSTEM_T3,
};
use crate::yaml::Value;

/// A hex field written as `0x293`, zero-padded to the field's digits.
fn hex(v: u32, digits: usize) -> Value {
    Value::Raw(format!("0x{v:0digits$X}"))
}

fn int(v: impl Into<i64>) -> Value {
    Value::Int(v.into())
}

/// A frequency field: the text, or 0 for none.
fn freq(hz: u64) -> Value {
    if hz == 0 {
        Value::Int(0)
    } else {
        Value::str(frequency_text(hz))
    }
}

fn opt<T: Into<i64>>(v: Option<T>) -> Value {
    v.map_or(Value::Null, |x| Value::Int(x.into()))
}

fn map(items: Vec<(&str, Value)>) -> Value {
    Value::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// The system type's short name, as the block names it.
fn type_short(t: u8) -> Option<&'static str> {
    match t {
        SYSTEM_T3 => Some("T3"),
        SYSTEM_P25_TRUNK => Some("P25T"),
        SYSTEM_NXDN => Some("NXDN"),
        _ => None,
    }
}

fn kind_short(k: u8) -> Option<&'static str> {
    match k {
        KIND_T3 => Some("T3"),
        KIND_P25 => Some("P25"),
        KIND_P25_TRUNK => Some("P25T"),
        KIND_NXDN => Some("NXDN"),
        _ => None,
    }
}

/// A system's `settings` map, the keys of a new system of its type in order.
fn settings_of(f: &SystemFields) -> Option<Value> {
    Some(match f {
        SystemFields::P25Trunk(x) => map(vec![
            ("wacn", x.wacn.map_or(Value::Null, |v| hex(v, 5))),
            ("sysid", x.sysid.map_or(Value::Null, |v| hex(u32::from(v), 3))),
            ("ccNac", x.cc_nac.map_or(Value::Null, |v| hex(u32::from(v), 3))),
            ("skipEnc", Value::Bool(x.skip_enc)),
            ("planOnly", Value::Bool(x.plan_only)),
            ("noReg", Value::Bool(x.no_reg)),
            ("holdS", int(x.hold_s)),
            ("roamDbm", int(x.roam_dbm)),
        ]),
        SystemFields::T3(x) => map(vec![
            ("model", int(x.model)),
            ("net", int(x.net)),
            ("category", Value::str(if x.category_b { "b" } else { "a" })),
            ("colorCode", int(x.color_code)),
            ("allCalls", Value::Bool(x.all_calls)),
            ("emergencyAny", Value::Bool(x.emergency_any)),
            ("followAll", Value::Bool(x.follow_all)),
            ("learnSystem", Value::Bool(x.learn_system)),
            ("privateCalls", Value::Bool(x.private_calls)),
            ("registerTalkgroups", Value::Bool(x.register_talkgroups)),
            ("acceptAls", Value::Bool(x.accept_als)),
            ("acceptKill", Value::Bool(x.accept_kill)),
            ("noRoaming", Value::Bool(x.no_roaming)),
            ("acceptStun", Value::Bool(x.accept_stun)),
            ("ipGateway", Value::Bool(x.ip_gateway)),
            ("fallbackHold", int(x.fallback_hold)),
            ("planBaseHz", freq(u64::from(x.plan_base_hz))),
            ("planFirstLcn", int(x.plan_first_lcn)),
            ("planSep", int(x.plan_sep)),
            ("planTxLow", Value::Bool(x.plan_tx_low)),
            ("planNoLearn", Value::Bool(x.plan_no_learn)),
            ("planSplitHz", freq(u64::from(x.plan_split_hz))),
            ("leaveDbm", int(x.leave_dbm)),
            ("powerSave", int(x.power_save)),
            ("sites", Value::flow(Value::List(x.sites.iter().map(|s| int(*s)).collect()))),
        ]),
        SystemFields::Nxdn => map(vec![]),
        // AES systems and channels stay out of a qdmr file: it carries no keys, and qdmr has no such mode
        SystemFields::Aes { .. } | SystemFields::Unknown => return None,
    })
}

/// An entry as a flow map: `cls`, `group` (a class with groups), `flags` (when set), `control`, then the class's fields.
fn entry_of(e: &SysEntry) -> Option<Value> {
    let mut out: Vec<(&str, Value)> = vec![("cls", int(e.cls))];
    let groups = !matches!(e.body, EntryBody::T3Channel { .. });
    if groups {
        out.push(("group", int(e.group)));
    }
    if e.flags != 0 {
        out.push(("flags", int(e.flags)));
    }
    out.push(("control", Value::Bool(e.control)));
    match &e.body {
        EntryBody::P25Control { iden, chan_no, rx_hz, tx_hz } => {
            out.push(("iden", int(*iden)));
            out.push(("chanNo", int(*chan_no)));
            out.push(("rxHz", freq(u64::from(*rx_hz))));
            out.push(("txHz", freq(u64::from(*tx_hz))));
        }
        EntryBody::P25Site { rfss, site } => {
            out.push(("rfss", opt(*rfss)));
            out.push(("site", opt(*site)));
        }
        EntryBody::P25Plan { base_hz, spacing_hz, tx_offset_hz, bw_hz, tdma, slots } => {
            out.push(("baseHz", freq(u64::from(*base_hz))));
            out.push(("spacingHz", int(*spacing_hz)));
            out.push(("txOffsetHz", int(*tx_offset_hz)));
            out.push(("bwHz", int(*bw_hz)));
            out.push(("tdma", Value::Bool(*tdma)));
            out.push(("slots", int(*slots)));
        }
        EntryBody::T3Channel { lcn, rx_hz, tx_hz, cc } => {
            out.push(("chan", int(*lcn)));
            out.push(("rxHz", freq(u64::from(*rx_hz))));
            out.push(("txHz", freq(u64::from(*tx_hz))));
            out.push(("cc", opt(*cc)));
        }
        EntryBody::Other => return None,
    }
    Some(Value::flow(map(out)))
}

fn sharing_text(s: u8) -> Value {
    match s {
        SHARE_OFF => Value::str("off"),
        SHARE_IDENTITY => Value::str("identity"),
        SHARE_WHOLE => Value::str("whole"),
        n => int(n),
    }
}

/// A channel's parameter block as a flow map, or `None` for a kind this editor doesn't know.
fn params_of(p: &Params) -> Option<Value> {
    Some(Value::flow(match p {
        Params::P25(x) => map(vec![
            ("squelch", int(x.squelch)),
            ("rxNac", hex(u32::from(x.rx_nac), 3)),
            ("txNac", hex(u32::from(x.tx_nac), 3)),
        ]),
        Params::Nxdn(x) => {
            let mut v = vec![
                ("rate9600", Value::Bool(x.rate9600)),
                ("squelch", int(x.squelch)),
                ("rxRan", int(x.rx_ran)),
                ("txRan", int(x.tx_ran)),
            ];
            if let Some(id) = x.tx_key_id {
                v.push(("txKeyId", int(id)));
            }
            map(v)
        }
        Params::P25Trunk(x) => map(vec![("followAll", Value::Bool(x.follow_all))]),
        Params::T3 => map(vec![]),
        Params::Aes(_) | Params::Unknown => return None,
    }))
}

/// What the block writes for a codeplug: the top-level block and, per channel, its own.
pub struct ModesExport<'a> {
    pub top: Option<Value>,
    systems: Vec<(&'a System, String)>,
}

/// The block's parts for a codeplug. `chan_ref` names a channel by its id in the file (`None`: not in the file).
pub fn export_modes<'a>(
    cp: &'a Codeplug,
    chan_ref: &dyn Fn(u16) -> Option<String>,
    notes: &mut Notes,
) -> ModesExport<'a> {
    let mut kept: Vec<&System> = Vec::new();
    for s in &cp.systems {
        if type_short(s.type_id).is_some() && !matches!(s.fields, SystemFields::Unknown) {
            kept.push(s);
        } else {
            add(notes, Note::ModeUnknown, 1);
        }
    }
    let systems: Vec<(&System, String)> =
        kept.iter().enumerate().map(|(i, s)| (*s, format!("sys{}", i + 1))).collect();
    let keys = kept.iter().filter(|s| s.secret.as_ref().is_some_and(|v| !v.is_empty()) || !s.traffic_keys.is_empty()).count();
    if keys > 0 {
        add(notes, Note::ModeKeys, keys);
    }
    let fallback = |f: &Fallback| match f {
        Fallback::Channel(n) => chan_ref(*n),
        _ => None,
    };
    let mut out: Vec<Value> = Vec::new();
    for (s, id) in &systems {
        let entries: Vec<Value> = s
            .entries
            .iter()
            .filter_map(|e| {
                let v = entry_of(e);
                if v.is_none() {
                    add(notes, Note::ModeUnknown, 1);
                }
                v
            })
            .collect();
        let mut m: Vec<(&str, Value)> = vec![
            ("id", Value::str(id.clone())),
            ("type", Value::str(type_short(s.type_id).unwrap_or("?"))),
            ("name", Value::str(s.name.clone())),
            ("transmit", Value::Bool(s.tx)),
            ("ownId", int(s.own_id)),
            ("authenticates", Value::Bool(s.needs_key)),
            ("sharing", sharing_text(s.sharing)),
        ];
        if let Some(f) = fallback(&s.fallback) {
            m.push(("fallback", Value::str(f)));
        }
        m.push(("settings", Value::flow(settings_of(&s.fields).unwrap_or_else(|| map(vec![])))));
        m.push((
            "entries",
            Value::List(entries),
        ));
        out.push(map(m));
    }
    // the radio-wide records: those set away from their defaults
    let mut settings: Vec<(&str, Value)> = Vec::new();
    if cp.mode_settings.p25_unit_id != 0 {
        settings.push(("P25", Value::flow(map(vec![("unitId", int(cp.mode_settings.p25_unit_id))]))));
    }
    if cp.mode_settings.nxdn_unit_id != 0 {
        settings.push(("NXDN", Value::flow(map(vec![("unitId", int(cp.mode_settings.nxdn_unit_id))]))));
    }
    let top = if out.is_empty() && settings.is_empty() {
        None
    } else {
        let mut t: Vec<(&str, Value)> = Vec::new();
        if !out.is_empty() {
            t.push(("systems", Value::List(out)));
        }
        if !settings.is_empty() {
            t.push(("modeSettings", map(settings)));
        }
        Some(map(t))
    };
    ModesExport { top, systems }
}

impl ModesExport<'_> {
    /// A channel's block, or `None` when it has nothing to say.
    pub fn channel(
        &self,
        c: &Channel,
        chan_ref: &dyn Fn(u16) -> Option<String>,
        notes: &mut Notes,
    ) -> Option<Value> {
        let mut b: Vec<(&str, Value)> = Vec::new();
        if let Some(t) = &c.tagged {
            match (kind_short(t.kind), params_of(&t.params)) {
                (Some(short), Some(settings)) => {
                    b.push(("mode", Value::str(short)));
                    if t.system != 0xFF {
                        if let Some((_, id)) = self.systems.iter().find(|(s, _)| s.number == u16::from(t.system) + 1) {
                            b.push(("system", Value::str(id.clone())));
                        }
                    }
                    b.push(("transmit", Value::Bool(t.tx)));
                    if t.base_own {
                        b.push(("ownFrequencies", Value::Bool(true)));
                    }
                    if matches!(t.kind, KIND_P25_TRUNK | KIND_T3) && t.no_fallback {
                        b.push(("noFallback", Value::Bool(true)));
                    }
                    if let Fallback::Channel(n) = &t.fallback {
                        if let Some(r) = chan_ref(*n) {
                            b.push(("fallback", Value::str(r)));
                        }
                    }
                    // the parameter block, left out when it holds nothing
                    if !matches!(&settings, Value::Flow(v) if matches!(&**v, Value::Map(m) if m.is_empty())) {
                        b.push(("settings", settings));
                    }
                }
                _ => add(notes, Note::ModeUnknown, 1),
            }
        }
        if c.accept_bad_crc {
            b.push(("acceptInvalidCrc", Value::Bool(true)));
        }
            (!b.is_empty()).then(|| Value::flow(map(b)))
    }
}

// --- Import ---

/// JavaScript's `Number(v)` for a scalar, when it is finite (`""` is not a number here, as in the web editor).
fn to_number(v: &Value) -> Option<f64> {
    let n = match v.untag() {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Str(s) => {
            let t = s.trim();
            if s.is_empty() {
                return None;
            }
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().ok()?
            }
        }
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// A hex field as read: a number (`0x293` or `659`) or hex text (`"293"`, `"0x293"`); `None` for none.
fn hex_in(v: &Value) -> Option<u32> {
    match v.untag() {
        Value::Int(i) => u32::try_from(*i).ok(),
        Value::Float(f) if f.fract() == 0.0 && *f >= 0.0 && *f <= f64::from(u32::MAX) => Some(*f as u32),
        Value::Str(s) => {
            let t = s.trim();
            let t = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
            (!t.is_empty() && t.chars().all(|c| c.is_ascii_hexdigit())).then(|| u32::from_str_radix(t, 16).ok()).flatten()
        }
        _ => None,
    }
}

/// A field the way `fieldsIn` reads it, for a field of a given default: the value in the map under `key`, if any.
struct Given<'a>(Option<&'a Value>);

impl<'a> Given<'a> {
    fn new(v: Option<&'a Value>) -> Self {
        Given(v.filter(|v| v.is_map()))
    }
    fn get(&self, key: &str) -> Option<&'a Value> {
        self.0.and_then(|g| g.get(key)).map(Value::untag)
    }
    /// A boolean: only `true` is true; a key that is not there keeps the default.
    fn flag(&self, key: &str, d: bool) -> bool {
        self.get(key).map_or(d, |v| matches!(v, Value::Bool(true)))
    }
    /// A number (`null` reads as none): the default when the value is no number.
    fn num(&self, key: &str, d: Option<f64>) -> Option<f64> {
        match self.get(key) {
            None => d,
            Some(Value::Null) => None,
            Some(v) => to_number(v).or(d),
        }
    }
    fn int<T: TryFrom<i64> + Copy + Into<i64>>(&self, key: &str, d: T) -> T {
        let dd: i64 = d.into();
        let n = self.num(key, Some(dd as f64)).unwrap_or(0.0);
        T::try_from(n as i64).unwrap_or(d)
    }
    /// A frequency: `0` is none; text is parsed; the default when it is neither.
    fn freq(&self, key: &str, d: u64) -> u64 {
        match self.get(key) {
            None => d,
            Some(Value::Int(0)) => 0,
            Some(v) => crate::qdmr::parse_frequency(Some(v)).unwrap_or(d),
        }
    }
    /// A hex field that may be "any" (`~`).
    fn hex_opt(&self, key: &str, d: Option<u32>) -> Option<u32> {
        match self.get(key) {
            None | Some(Value::Null) => d,
            Some(v) => hex_in(v).or(d),
        }
    }
    fn text(&self, key: &str, d: &str) -> String {
        match self.get(key) {
            None | Some(Value::Null) => d.to_string(),
            Some(v) => crate::yaml::text_of(v),
        }
    }
}

fn system_fields_in(type_id: u8, settings: Option<&Value>) -> SystemFields {
    let g = Given::new(settings);
    match type_id {
        SYSTEM_P25_TRUNK => {
            let d = crate::systems::P25Trunk::default();
            SystemFields::P25Trunk(crate::systems::P25Trunk {
                wacn: g.hex_opt("wacn", d.wacn),
                sysid: g.hex_opt("sysid", d.sysid.map(u32::from)).map(|v| v as u16),
                cc_nac: g.hex_opt("ccNac", d.cc_nac.map(u32::from)).map(|v| v as u16),
                skip_enc: g.flag("skipEnc", d.skip_enc),
                plan_only: g.flag("planOnly", d.plan_only),
                no_reg: g.flag("noReg", d.no_reg),
                hold_s: g.int("holdS", i64::from(d.hold_s)) as u8,
                roam_dbm: g.int("roamDbm", i64::from(d.roam_dbm)) as i8,
            })
        }
        SYSTEM_T3 => {
            let d = crate::systems::T3System::default();
            let sites = match g.get("sites") {
                Some(Value::List(l)) => l.iter().filter_map(to_number).map(|n| n as u16).collect(),
                _ => d.sites.clone(),
            };
            SystemFields::T3(crate::systems::T3System {
                model: g.int("model", i64::from(d.model)) as u8,
                net: g.int("net", i64::from(d.net)) as u16,
                category_b: g.text("category", if d.category_b { "b" } else { "a" }) == "b",
                color_code: g.int("colorCode", i64::from(d.color_code)) as u8,
                all_calls: g.flag("allCalls", d.all_calls),
                emergency_any: g.flag("emergencyAny", d.emergency_any),
                follow_all: g.flag("followAll", d.follow_all),
                learn_system: g.flag("learnSystem", d.learn_system),
                private_calls: g.flag("privateCalls", d.private_calls),
                register_talkgroups: g.flag("registerTalkgroups", d.register_talkgroups),
                accept_als: g.flag("acceptAls", d.accept_als),
                accept_kill: g.flag("acceptKill", d.accept_kill),
                no_roaming: g.flag("noRoaming", d.no_roaming),
                accept_stun: g.flag("acceptStun", d.accept_stun),
                ip_gateway: g.flag("ipGateway", d.ip_gateway),
                fallback_hold: g.int("fallbackHold", i64::from(d.fallback_hold)) as u8,
                plan_base_hz: g.freq("planBaseHz", u64::from(d.plan_base_hz)) as u32,
                plan_first_lcn: g.int("planFirstLcn", i64::from(d.plan_first_lcn)) as u16,
                plan_sep: g.int("planSep", i64::from(d.plan_sep)) as u8,
                plan_tx_low: g.flag("planTxLow", d.plan_tx_low),
                plan_no_learn: g.flag("planNoLearn", d.plan_no_learn),
                plan_split_hz: g.freq("planSplitHz", u64::from(d.plan_split_hz)) as u32,
                leave_dbm: g.int("leaveDbm", i64::from(d.leave_dbm)) as i8,
                power_save: g.int("powerSave", i64::from(d.power_save)) as u8,
                sites,
            })
        }
        SYSTEM_NXDN => SystemFields::Nxdn,
        _ => SystemFields::Unknown,
    }
}

/// An entry of a system of `type_id` from its map, or `None` for a class the type doesn't have.
fn entry_in(type_id: u8, e: &Value) -> Option<SysEntry> {
    let g = Given::new(Some(e));
    let cls = g.get("cls").and_then(to_number).unwrap_or(0.0) as i64;
    let (group0, control0, body) = match (type_id, cls) {
        (SYSTEM_P25_TRUNK, 0) => (
            0u8,
            true,
            EntryBody::P25Control {
                iden: g.int("iden", 0i64) as u8,
                chan_no: g.int("chanNo", 0i64) as u16,
                rx_hz: g.freq("rxHz", 0) as u32,
                tx_hz: g.freq("txHz", 0) as u32,
            },
        ),
        (SYSTEM_P25_TRUNK, 1) => (
            0,
            false,
            EntryBody::P25Site {
                rfss: g.num("rfss", None).map(|n| n as u8),
                site: g.num("site", None).map(|n| n as u8),
            },
        ),
        (SYSTEM_P25_TRUNK, 2) => (
            0,
            false,
            EntryBody::P25Plan {
                base_hz: g.freq("baseHz", 0) as u32,
                spacing_hz: g.int("spacingHz", 12500i64) as u32,
                tx_offset_hz: g.int("txOffsetHz", 0i64) as i32,
                bw_hz: g.int("bwHz", 12500i64) as u32,
                tdma: g.flag("tdma", false),
                slots: g.int("slots", 2i64) as u8,
            },
        ),
        (SYSTEM_T3, 0) => (
            0xFF,
            true,
            EntryBody::T3Channel {
                lcn: g.int("chan", 1i64) as u16,
                rx_hz: g.freq("rxHz", 0) as u32,
                tx_hz: g.freq("txHz", 0) as u32,
                cc: g.num("cc", None).map(|n| n as u8),
            },
        ),
        _ => return None,
    };
    Some(SysEntry {
        cls: cls as u8,
        group: g.int("group", i64::from(group0)) as u8,
        control: g.flag("control", control0),
        flags: g.int("flags", 0i64) as u8,
        body,
        raw: Vec::new(),
    })
}

/// The block's systems and channel modes into the codeplug, after its channels are in place. `top`: the file's
/// top-level block; `chans`: the index in `cp.channels` of every channel with its own block; `by_id` gives a channel's
/// number from its id in the file. A system with the identity (or, for a type without one, the type and name) of one
/// the codeplug had keeps that system's keys, which the file never holds. Returns false when the file has no block
/// at all: the codeplug's systems and radio-wide records stay as they were.
pub fn import_modes(
    cp: &mut Codeplug,
    top: Option<&Value>,
    chans: &[(usize, Option<&Value>)],
    by_id: &dyn Fn(&str) -> Option<u16>,
    notes: &mut Notes,
) -> bool {
    let top = top.filter(|t| t.is_map() || matches!(t.untag(), Value::Map(_)));
    for (i, b) in chans {
        if let (Some(c), Some(b)) = (cp.channels.get_mut(*i), b) {
            if matches!(b.get("acceptInvalidCrc").map(Value::untag), Some(Value::Bool(true))) {
                c.accept_bad_crc = c.mode == crate::records::Mode::Dmr;
            }
        }
    }
    let any = top.is_some() || chans.iter().any(|(_, b)| b.is_some());
    let ref_of = |r: Option<&Value>| -> Option<u16> {
        let r = r?;
        if matches!(r.untag(), Value::Null) {
            return None;
        }
        by_id(&crate::yaml::text_of(r.untag())).filter(|n| *n != 0)
    };
    if !any {
        // the systems stay; a fallback to a channel the file replaced can't
        for s in &mut cp.systems {
            if matches!(s.fallback, Fallback::Channel(_)) {
                s.fallback = Fallback::None;
            }
        }
        return false;
    }

    let had = cp.systems.clone();
    let same = |s: &System| {
        let id = s.identity();
        had.iter().find(|x| {
            x.type_id == s.type_id
                && match &id {
                    Some(id) => x.identity().as_ref() == Some(id),
                    None => x.name == s.name,
                }
        })
    };
    let mut systems: Vec<System> = Vec::new();
    let mut sys_by_id: Vec<(String, usize)> = Vec::new();
    for x in top.and_then(|t| t.get("systems")).map_or(&[][..], Value::as_list) {
        let row = x
            .get("type")
            .map(|t| crate::yaml::text_of(t.untag()))
            .and_then(|t| [SYSTEM_T3, SYSTEM_P25_TRUNK, SYSTEM_NXDN].into_iter().find(|id| type_short(*id) == Some(t.as_str())));
        let Some(type_id) = row else {
            add(notes, Note::ModeUnknown, 1);
            continue;
        };
        let name: String = match x.get("name").map(Value::untag) {
            None | Some(Value::Null) => String::new(),
            Some(v) => crate::yaml::text_of(v),
        }
        .chars()
        .take(16)
        .collect();
        let mut s = System::new(type_id, &name);
        if let Some(t) = x.get("transmit").filter(|t| !matches!(t.untag(), Value::Null)) {
            s.tx = matches!(t.untag(), Value::Bool(true));
        }
        s.own_id = x.get("ownId").and_then(to_number).map_or(0, |n| if n > 0.0 { n as u32 } else { 0 });
        s.needs_key = matches!(x.get("authenticates").map(Value::untag), Some(Value::Bool(true)));
        if let Some(sh) = x.get("sharing").map(Value::untag).filter(|v| !matches!(v, Value::Null)) {
            match sh {
                Value::Str(t) if t == "off" => s.sharing = SHARE_OFF,
                Value::Str(t) if t == "identity" => s.sharing = SHARE_IDENTITY,
                Value::Str(t) if t == "whole" => s.sharing = SHARE_WHOLE,
                Value::Int(n) if (0..=255).contains(n) => s.sharing = *n as u8,
                _ => {}
            }
        }
        s.fallback = ref_of(x.get("fallback")).map_or(Fallback::None, Fallback::Channel);
        s.fields = system_fields_in(type_id, x.get("settings"));
        for e in x.get("entries").map_or(&[][..], Value::as_list) {
            match entry_in(type_id, e) {
                Some(en) => s.entries.push(en),
                None => add(notes, Note::ModeUnknown, 1),
            }
        }
        if let Some(was) = same(&s) {
            s.secret = was.secret.clone();
            s.traffic_keys = was.traffic_keys.clone();
        }
        sys_by_id.push((x.get("id").map(|v| crate::yaml::text_of(v.untag())).unwrap_or_default(), systems.len()));
        systems.push(s);
    }
    for (i, s) in systems.iter_mut().enumerate() {
        s.number = (i + 1) as u16;
    }
    cp.systems = systems;
    cp.orphan_entries.clear();

    // the radio-wide records: a kind the block leaves out takes its defaults
    let given = Given::new(top.and_then(|t| t.get("modeSettings")));
    cp.mode_settings.p25_unit_id =
        Given::new(given.get("P25")).int("unitId", 0i64).clamp(0, i64::from(u32::MAX)) as u32;
    cp.mode_settings.nxdn_unit_id =
        Given::new(given.get("NXDN")).int("unitId", 0i64).clamp(0, i64::from(u16::MAX)) as u16;

    for (i, b) in chans {
        let Some(c) = cp.channels.get_mut(*i) else { continue };
        c.tagged = None;
        let Some(b) = b else { continue };
        let Some(mode) = b.get("mode").map(Value::untag).filter(|m| !matches!(m, Value::Null)) else { continue };
        let mode = crate::yaml::text_of(mode);
        let Some(kind) = [KIND_T3, KIND_P25, KIND_P25_TRUNK, KIND_NXDN].into_iter().find(|k| kind_short(*k) == Some(mode.as_str()))
        else {
            add(notes, Note::ModeUnknown, 1);
            continue;
        };
        let sys_ref = b.get("system").map(Value::untag).filter(|v| !matches!(v, Value::Null));
        let system = sys_ref.and_then(|r| {
            let id = crate::yaml::text_of(r);
            sys_by_id.iter().rev().find(|(k, _)| *k == id).map(|(_, i)| *i)
        });
        let sys_byte = system.map_or(0xFF, |i| i as u8);
        let default_tx = matches!(kind, KIND_P25 | KIND_NXDN | KIND_T3);
        let tx = match b.get("transmit").map(Value::untag) {
            None | Some(Value::Null) => default_tx,
            Some(v) => matches!(v, Value::Bool(true)),
        };
        let g = Given::new(b.get("settings"));
        let params = match kind {
            KIND_P25 => {
                let d = crate::modes::P25Conv::default();
                Params::P25(crate::modes::P25Conv {
                    squelch: g.int("squelch", i64::from(d.squelch)) as u8,
                    rx_nac: g.hex_opt("rxNac", Some(u32::from(d.rx_nac))).map_or(d.rx_nac, |v| v as u16),
                    tx_nac: g.hex_opt("txNac", Some(u32::from(d.tx_nac))).map_or(d.tx_nac, |v| v as u16),
                })
            }
            KIND_NXDN => {
                let d = crate::modes::NxdnConv::default();
                Params::Nxdn(crate::modes::NxdnConv {
                    rate9600: g.flag("rate9600", d.rate9600),
                    squelch: g.int("squelch", i64::from(d.squelch)) as u8,
                    rx_ran: g.int("rxRan", i64::from(d.rx_ran)) as u8,
                    tx_ran: g.int("txRan", i64::from(d.tx_ran)) as u8,
                    tx_key_id: g.get("txKeyId").and_then(to_number).filter(|n| n.fract() == 0.0).map(|n| n as u8),
                })
            }
            KIND_P25_TRUNK => Params::P25Trunk(crate::modes::P25TrunkChan { follow_all: g.flag("followAll", false) }),
            _ => Params::T3,
        };
        let fallback = ref_of(b.get("fallback")).map_or(Fallback::None, Fallback::Channel);
        let flag_b = |k: &str| matches!(b.get(k).map(Value::untag), Some(Value::Bool(true)));
        c.tagged = Some(crate::modes::Tagged {
            kind,
            system: sys_byte,
            tx,
            base_own: flag_b("ownFrequencies"),
            fallback,
            no_fallback: matches!(kind, KIND_P25_TRUNK | KIND_T3) && flag_b("noFallback"),
            params,
            raw: Vec::new(),
        });
        if sys_ref.is_some() && system.is_none() {
            add(notes, Note::ModeUnknown, 1);
        }
    }
    // the stand-ins: a channel of a kind is a receive-only DMR channel on its system's first control channel
    let systems = cp.systems.clone();
    for c in &mut cp.channels {
        if c.tagged.is_some() {
            crate::modes::stand_in(c, &systems);
        }
    }
    true
}
